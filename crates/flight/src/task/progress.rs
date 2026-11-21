//! Following a flight through a task: which turnpoints are reached, when
//! the clock started and stopped, and where to fly next.
//!
//! The rules are those of the FAI Sporting Code, Section 7F (2024):
//!
//! - A turnpoint is reached by crossing the edge of its cylinder after
//!   the one before it was reached, in either direction (6.2.1, 8.3).
//!   Scoring allows a band of tolerance around the edge (8.1); an
//!   instrument must not count on it, so here only a fix on the far side
//!   of the edge itself counts.
//! - The start is the last crossing of the start cylinder before the next
//!   turnpoint is reached (6.3.1), and it counts only once the first gate
//!   is open (6.3.3). In a race the clock starts with the last gate that
//!   opened before that crossing; in an elapsed-time task, with the
//!   crossing itself (6.3.3, 6.3.4, 8.3.1).
//! - A goal line is reached by crossing it or by entering the half circle
//!   behind it (6.2.3.1).
//!
//! Not followed: going back for a later start after rounding turnpoints
//! (7.1), and a task stopped by its director (12.3). A pilot who restarts
//! steps the task back by hand.
//!
//! A fix that says "started" one second early would cost a pilot the
//! task, so a crossing counts as a start only if both of its fixes are
//! at or after the gate.

use venti_domain::GeoPoint;

use super::route::{self, Zone};
use super::{Measured, StartKind, Task};

/// The speed section's average speed is given once the clock has run
/// this long, seconds.
const MIN_AVERAGE_S: i64 = 60;

/// Where the glider is at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    /// The moment, seconds since 1970-01-01 UTC.
    pub time_unix_s: i64,
    /// The position.
    pub position: GeoPoint,
}

/// How far the flight has come in its task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The task has a start, and it has not been taken.
    BeforeStart,
    /// On the way round the turnpoints.
    OnTask,
    /// The speed section is flown and the goal still ahead.
    SpeedSectionDone,
    /// In goal.
    InGoal,
}

/// Something that happened at a fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskEvent {
    /// The start was taken, or taken again. The clock runs from
    /// `clock_from_unix_s`.
    Started {
        /// When the pilot's clock started.
        clock_from_unix_s: i64,
    },
    /// A turnpoint was reached.
    Reached {
        /// Its index in the task.
        index: usize,
    },
    /// The speed section is flown.
    SpeedSectionDone {
        /// How long it took, seconds.
        time_s: i64,
    },
    /// The goal is reached.
    Goal,
}

/// The turnpoint to fly to.
#[derive(Debug, Clone, PartialEq)]
pub struct NextTurnpoint {
    /// Its index in the task.
    pub index: usize,
    /// Its name.
    pub name: String,
    /// The point to fly to: where the shortest way through the rest of
    /// the task touches it.
    pub aim: GeoPoint,
    /// How far that point is, metres.
    pub distance_m: f64,
    /// The direction to it, degrees clockwise from north.
    pub bearing_deg: f64,
    /// How far the turnpoint's centre is, metres. Compared with the
    /// radius it says how far the edge is.
    pub centre_distance_m: f64,
    /// The radius of its cylinder, metres.
    pub radius_m: f64,
    /// Whether the glider is inside the cylinder now. Then the edge is
    /// reached by flying out.
    pub inside: bool,
    /// The height of the ground at the turnpoint, metres above sea level.
    pub altitude_m: f64,
}

/// The start, while it is still to be taken or can be taken again.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StartStatus {
    /// The gate that matters now: the first while none is open, then the
    /// last one that opened. `None` for a start that is always open.
    pub gate_unix_s: Option<i64>,
    /// Seconds until that gate opens; negative once it is open.
    pub opens_in_s: Option<i64>,
    /// The next gate after it, for a task with several.
    pub next_gate_unix_s: Option<i64>,
    /// How far the edge of the start cylinder is along the shortest way,
    /// metres.
    pub distance_m: f64,
    /// The speed over the ground that brings the glider to the edge just
    /// as the gate opens, metres per second. `None` once it is open.
    pub speed_to_gate_ms: Option<f64>,
}

/// The state of the task for the cockpit.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskStatus {
    /// How far the flight has come.
    pub phase: Phase,
    /// The turnpoint to fly to. `None` in goal.
    pub next: Option<NextTurnpoint>,
    /// The shortest way still to fly: from the glider to each remaining
    /// turnpoint's edge.
    pub way: Vec<GeoPoint>,
    /// Its length to the goal, metres.
    pub to_goal_m: f64,
    /// Its length to the end of the speed section, metres, while that is
    /// still ahead.
    pub to_end_of_speed_m: Option<f64>,
    /// How much of the speed section is flown, from 0 to 1.
    pub speed_section_part: Option<f64>,
    /// The start, until the turnpoint after it is reached.
    pub start: Option<StartStatus>,
    /// When the pilot's clock started.
    pub clock_from_unix_s: Option<i64>,
    /// How long the speed section took, seconds, once it is flown.
    pub speed_section_time_s: Option<i64>,
    /// The speed made along the speed section since the clock started,
    /// metres per second: the part of its shortest way flown, over the
    /// time on the clock. Once the section is flown, its length over its
    /// time. `None` before the start and for the first minute after it,
    /// when a few metres make a speed of nothing.
    pub speed_section_average_ms: Option<f64>,
    /// When the goal was reached.
    pub goal_unix_s: Option<i64>,
    /// When the task ends.
    pub deadline_unix_s: Option<i64>,
    /// When each turnpoint was reached, in the task's order.
    pub reached_unix_s: Vec<Option<i64>>,
}

/// A flight in its task.
#[derive(Debug, Clone)]
pub struct TaskFlight {
    task: Task,
    measured: Measured,
    /// A moment of the flight, which says which day the task's times of
    /// day mean.
    anchor_unix_s: i64,
    /// The turnpoint to reach next; the number of turnpoints in goal.
    next: usize,
    reached: Vec<Option<i64>>,
    clock_from: Option<i64>,
    last: Option<Fix>,
}

impl TaskFlight {
    /// Begins a task. `anchor_unix_s` is a moment of the flight's day:
    /// when the task was set up in the cockpit, or the first fix.
    pub fn new(task: Task, anchor_unix_s: i64) -> Self {
        let measured = task.measure();
        let next = task.first_to_reach();
        let reached = vec![None; task.turnpoints.len()];
        Self {
            task,
            measured,
            anchor_unix_s,
            next,
            reached,
            clock_from: None,
            last: None,
        }
    }

    /// The task.
    pub fn task(&self) -> &Task {
        &self.task
    }

    /// The task measured from its launch.
    pub fn measured(&self) -> &Measured {
        &self.measured
    }

    /// The index of the turnpoint to reach next. `None` in goal.
    pub fn next_index(&self) -> Option<usize> {
        (self.next < self.task.turnpoints.len()).then_some(self.next)
    }

    fn gates(&self) -> Vec<i64> {
        self.task
            .start
            .iter()
            .flat_map(|start| &start.gates)
            .map(|gate| gate.near(self.anchor_unix_s))
            .collect()
    }

    fn deadline(&self) -> Option<i64> {
        self.task
            .goal
            .deadline
            .map(|deadline| deadline.near(self.anchor_unix_s))
    }

    /// Takes in the next fix and says what happened at it.
    pub fn update(&mut self, fix: Fix) -> Vec<TaskEvent> {
        let mut events = Vec::new();
        let previous = self.last.replace(fix);
        if self
            .deadline()
            .is_some_and(|deadline| fix.time_unix_s > deadline)
        {
            return events;
        }
        let start = self.task.start_index();
        if let Some(zone) = self.task.zone(self.next)
            && self.reaches(&zone, previous, fix)
        {
            let index = self.next;
            if Some(index) == start {
                if self.start_counts(previous, fix) {
                    self.take_start(index, fix, &mut events);
                    self.next = index + 1;
                }
            } else {
                self.reach(index, fix, &mut events);
                self.next = index + 1;
            }
        } else if let Some(start) = start
            && self.next == start + 1
            && let Some(zone) = self.task.zone(start)
            && self.reaches(&zone, previous, fix)
            && self.start_counts(previous, fix)
        {
            // Crossed the start again before the next turnpoint: this
            // crossing is the start now.
            self.take_start(start, fix, &mut events);
        }
        events
    }

    /// Whether the step from the fix before to this one reaches an area.
    fn reaches(&self, zone: &Zone, previous: Option<Fix>, fix: Fix) -> bool {
        let earth = self.task.earth;
        match *zone {
            Zone::Point { centre, reach_m } => earth.distance_m(centre, fix.position) <= reach_m,
            Zone::Cylinder { centre, radius_m } => {
                let Some(previous) = previous else {
                    return false;
                };
                let was_inside = earth.distance_m(centre, previous.position) <= radius_m;
                let is_inside = earth.distance_m(centre, fix.position) <= radius_m;
                was_inside != is_inside
            }
            Zone::Line {
                centre,
                half_length_m,
                inbound_deg,
            } => {
                // Metres past the line, and metres along it, of a place.
                let place = |position: GeoPoint| {
                    let course = earth.course(centre, position);
                    let off = (course.bearing_deg - inbound_deg).to_radians();
                    (course.distance_m * off.cos(), course.distance_m * off.sin())
                };
                let (past, along) = place(fix.position);
                // In the half circle behind the line.
                if past >= 0.0 && past.hypot(along) <= half_length_m {
                    return true;
                }
                // Or across the line itself since the last fix.
                let Some(previous) = previous else {
                    return false;
                };
                let (was_past, was_along) = place(previous.position);
                if was_past < 0.0 && past >= 0.0 {
                    let part = was_past / (was_past - past);
                    let crossed_at = was_along + part * (along - was_along);
                    return crossed_at.abs() <= half_length_m;
                }
                false
            }
        }
    }

    /// Whether a crossing of the start cylinder counts: the first gate
    /// was open at both of its fixes, and the start has not closed.
    fn start_counts(&self, previous: Option<Fix>, fix: Fix) -> bool {
        let earlier = previous.map_or(fix.time_unix_s, |previous| previous.time_unix_s);
        let open = self.gates().first().is_none_or(|first| earlier >= *first);
        let closed = self
            .task
            .start
            .as_ref()
            .and_then(|start| start.close)
            .is_some_and(|close| fix.time_unix_s > close.near(self.anchor_unix_s));
        open && !closed
    }

    fn take_start(&mut self, index: usize, fix: Fix, events: &mut Vec<TaskEvent>) {
        self.reached[index] = Some(fix.time_unix_s);
        let race = self
            .task
            .start
            .as_ref()
            .is_some_and(|start| start.kind == StartKind::Race);
        // In a race the clock starts with the last gate that opened
        // before the crossing; otherwise with the crossing.
        let gate = self
            .gates()
            .into_iter()
            .filter(|gate| *gate <= fix.time_unix_s)
            .max();
        let clock_from = match gate {
            Some(gate) if race => gate,
            _ => fix.time_unix_s,
        };
        self.clock_from = Some(clock_from);
        events.push(TaskEvent::Started {
            clock_from_unix_s: clock_from,
        });
        // A start that is also the end of the speed section or the goal
        // would be a task of no length; nothing more to say here.
    }

    fn reach(&mut self, index: usize, fix: Fix, events: &mut Vec<TaskEvent>) {
        self.reached[index] = Some(fix.time_unix_s);
        events.push(TaskEvent::Reached { index });
        if Some(index) == self.task.end_of_speed_index()
            && let Some(clock_from) = self.clock_from
        {
            events.push(TaskEvent::SpeedSectionDone {
                time_s: fix.time_unix_s - clock_from,
            });
        }
        if index == self.task.goal_index() {
            events.push(TaskEvent::Goal);
        }
    }

    /// Moves on to the next turnpoint without having reached this one,
    /// when the pilot says so.
    pub fn skip(&mut self) {
        if self.next < self.task.turnpoints.len() {
            self.next += 1;
        }
    }

    /// Steps back to the turnpoint before, forgetting that it and those
    /// after it were reached: for a pilot who goes back for a later
    /// start, or who skipped one too many.
    pub fn step_back(&mut self) {
        if self.next > self.task.first_to_reach() {
            self.next -= 1;
        }
        for reached in &mut self.reached[self.next..] {
            *reached = None;
        }
        if self
            .task
            .start_index()
            .is_some_and(|start| start >= self.next)
        {
            self.clock_from = None;
        }
    }

    /// The state of the task as seen from a position at a moment.
    pub fn status(&self, now: Fix) -> TaskStatus {
        let task = &self.task;
        let earth = task.earth;
        let start = task.start_index();
        let end = task.end_of_speed_index();
        let goal = task.goal_index();
        let goal_unix_s = self.reached.get(goal).copied().flatten();
        let end_reached = end.and_then(|end| self.reached[end]);

        let zones = task.zones_from(self.next);
        let only: Vec<Zone> = zones.iter().map(|(_, zone)| *zone).collect();
        let way = route::shortest(earth, now.position, &only);
        // How far along the way each remaining turnpoint is.
        let mut along = Vec::with_capacity(way.points.len());
        let mut sum = 0.0;
        let mut last = now.position;
        for point in &way.points {
            sum += earth.distance_m(last, *point);
            along.push(sum);
            last = *point;
        }
        let distance_to = |index: usize| {
            zones
                .iter()
                .position(|(zone_index, _)| *zone_index == index)
                .and_then(|place| along.get(place).copied())
        };

        let next = zones
            .first()
            .zip(way.points.first())
            .map(|((index, _), aim)| {
                let turnpoint = &task.turnpoints[*index];
                let to_aim = earth.course(now.position, *aim);
                let centre_distance_m = earth.distance_m(now.position, turnpoint.position);
                NextTurnpoint {
                    index: *index,
                    name: turnpoint.name.clone(),
                    aim: *aim,
                    distance_m: to_aim.distance_m,
                    bearing_deg: to_aim.bearing_deg,
                    centre_distance_m,
                    radius_m: turnpoint.radius_m,
                    inside: centre_distance_m <= turnpoint.radius_m,
                    altitude_m: turnpoint.altitude_m,
                }
            });

        let phase = if goal_unix_s.is_some() || self.next > goal {
            Phase::InGoal
        } else if end_reached.is_some() {
            Phase::SpeedSectionDone
        } else if start.is_some_and(|start| self.next <= start) {
            Phase::BeforeStart
        } else {
            Phase::OnTask
        };

        // The start matters until the turnpoint after it is reached.
        let start_status = start
            .filter(|start| self.next <= start + 1 && phase != Phase::InGoal)
            .map(|start| {
                let gates = self.gates();
                let opened = gates
                    .iter()
                    .copied()
                    .filter(|gate| *gate <= now.time_unix_s)
                    .max();
                let upcoming = gates
                    .iter()
                    .copied()
                    .filter(|gate| *gate > now.time_unix_s)
                    .min();
                let gate = opened.or(upcoming);
                let distance_m = distance_to(start).unwrap_or(0.0);
                let opens_in_s = gate.map(|gate| gate - now.time_unix_s);
                StartStatus {
                    gate_unix_s: gate,
                    opens_in_s,
                    next_gate_unix_s: opened.and(upcoming),
                    distance_m,
                    speed_to_gate_ms: opens_in_s
                        .filter(|seconds| *seconds > 0)
                        .map(|seconds| distance_m / seconds as f64),
                }
            });

        let to_end_of_speed_m = end.filter(|_| end_reached.is_none()).and_then(&distance_to);
        let speed_section_part = self.measured.speed_section_m.map(|length| {
            if end_reached.is_some() {
                1.0
            } else if phase == Phase::BeforeStart || length <= 0.0 {
                0.0
            } else {
                to_end_of_speed_m.map_or(0.0, |left| (1.0 - left / length).clamp(0.0, 1.0))
            }
        });

        let speed_section_average_ms = self.clock_from.and_then(|from| {
            let length = self.measured.speed_section_m?;
            let (flown_m, elapsed_s) = match end_reached {
                Some(end) => (length, end - from),
                None if phase == Phase::OnTask => (
                    (length - to_end_of_speed_m?).max(0.0),
                    now.time_unix_s - from,
                ),
                None => return None,
            };
            (elapsed_s >= MIN_AVERAGE_S).then(|| flown_m / elapsed_s as f64)
        });

        let mut line = Vec::with_capacity(way.points.len() + 1);
        if !way.points.is_empty() {
            line.push(now.position);
            line.extend(way.points.iter().copied());
        }
        TaskStatus {
            phase,
            next,
            way: line,
            to_goal_m: way.length_m,
            to_end_of_speed_m,
            speed_section_part,
            start: start_status,
            clock_from_unix_s: self.clock_from,
            speed_section_time_s: end_reached
                .zip(self.clock_from)
                .map(|(end, from)| end - from),
            speed_section_average_ms,
            goal_unix_s,
            deadline_unix_s: self.deadline(),
            reached_unix_s: self.reached.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geodesy::Earth;
    use crate::task::testing::{point, race, turnpoint};
    use crate::task::{Goal, GoalKind, Role, Start, TaskKind, TimeOfDay};

    /// 4 July 2026, 00:00 UTC.
    const MIDNIGHT: i64 = 1_783_123_200;

    fn at(hour: i64, minute: i64, second: i64) -> i64 {
        MIDNIGHT + hour * 3600 + minute * 60 + second
    }

    /// Flies in a straight line from one place to another at 15 m/s with
    /// a fix every second, and returns everything that happened with the
    /// moment it happened at.
    fn fly(
        flight: &mut TaskFlight,
        clock: &mut i64,
        from: GeoPoint,
        to: GeoPoint,
    ) -> Vec<(i64, TaskEvent)> {
        let earth = Earth::Wgs84;
        let course = earth.course(from, to);
        let steps = (course.distance_m / 15.0).ceil() as i64;
        let mut events = Vec::new();
        for step in 0..=steps {
            let position = earth.destination(
                from,
                course.bearing_deg,
                course.distance_m * step as f64 / steps as f64,
            );
            let fix = Fix {
                time_unix_s: *clock,
                position,
            };
            events.extend(
                flight
                    .update(fix)
                    .into_iter()
                    .map(|event| (fix.time_unix_s, event)),
            );
            *clock += 1;
        }
        events
    }

    fn here(clock: i64, position: GeoPoint) -> Fix {
        Fix {
            time_unix_s: clock,
            position,
        }
    }

    #[test]
    fn a_race_is_flown_from_the_start_gate_to_the_goal_line() {
        let task = race();
        let turnpoints = task.turnpoints.clone();
        let mut flight = TaskFlight::new(task, at(9, 30, 0));
        let launch = turnpoints[0].position;

        // Before the gate, waiting inside the start cylinder.
        let before = flight.status(here(at(10, 50, 0), launch));
        assert_eq!(before.phase, Phase::BeforeStart);
        assert_eq!(before.speed_section_average_ms, None);
        let start = before.start.unwrap();
        assert_eq!(start.gate_unix_s, Some(at(11, 0, 0)));
        assert_eq!(start.opens_in_s, Some(600));
        // The edge is 5 km away, the way the task goes on: to the west.
        assert!(
            (start.distance_m - 5_000.0).abs() < 0.5,
            "{}",
            start.distance_m
        );
        assert!((start.speed_to_gate_ms.unwrap() - 5_000.0 / 600.0).abs() < 0.01);
        let next = before.next.unwrap();
        assert_eq!((next.index, next.name.as_str()), (1, "START"));
        assert!(next.inside);
        assert!(
            (next.bearing_deg - 265.0).abs() < 10.0,
            "{}",
            next.bearing_deg
        );
        assert_eq!(before.speed_section_part, Some(0.0));
        assert_eq!(before.deadline_unix_s, Some(at(16, 0, 0)));
        // The whole task is still ahead, less nothing: the pilot is at
        // the launch.
        assert!((before.to_goal_m - flight.measured().length_m).abs() < 1.0);

        // Leaving the cylinder at 11:01: the clock runs from the gate.
        let mut clock = at(10, 55, 20);
        let west = turnpoints[2].position;
        let events = fly(&mut flight, &mut clock, launch, west);
        let started: Vec<_> = events
            .iter()
            .filter(|(_, event)| matches!(event, TaskEvent::Started { .. }))
            .collect();
        assert_eq!(started.len(), 1);
        let (crossed, TaskEvent::Started { clock_from_unix_s }) = started[0] else {
            unreachable!()
        };
        // 5 km at 15 m/s from 10:55:20.
        assert!(
            (*crossed - at(11, 0, 54)).abs() <= 2,
            "{}",
            crossed - MIDNIGHT
        );
        assert_eq!(*clock_from_unix_s, at(11, 0, 0));
        // And on into the cylinder of WEST.
        assert!(
            events
                .iter()
                .any(|(_, event)| *event == TaskEvent::Reached { index: 2 })
        );
        assert_eq!(flight.next_index(), Some(3));

        // On task, the start no longer matters.
        let on_task = flight.status(here(clock, west));
        assert_eq!(on_task.phase, Phase::OnTask);
        assert!(on_task.start.is_none());
        assert_eq!(on_task.clock_from_unix_s, Some(at(11, 0, 0)));
        let part = on_task.speed_section_part.unwrap();
        assert!(part > 0.1 && part < 0.4, "{part}");
        assert!(on_task.to_end_of_speed_m.unwrap() < on_task.to_goal_m);
        // The clock runs from the gate, and the glider crossed a minute
        // after it at 15 m/s: the average is a little under that.
        let average = on_task.speed_section_average_ms.unwrap();
        assert!(average > 10.0 && average < 15.0, "{average}");

        // To EAST, then to goal from the north-east.
        let east = turnpoints[3].position;
        let events = fly(&mut flight, &mut clock, west, east);
        assert!(
            events
                .iter()
                .any(|(_, event)| *event == TaskEvent::Reached { index: 3 })
        );
        let goal = turnpoints[5].position;
        // Over the line and a little beyond it.
        let beyond = Earth::Wgs84.destination(
            goal,
            Earth::Wgs84.course(goal, east).bearing_deg + 180.0,
            150.0,
        );
        let events = fly(&mut flight, &mut clock, east, beyond);
        let kinds: Vec<TaskEvent> = events.iter().map(|(_, event)| *event).collect();
        let done = kinds
            .iter()
            .find_map(|event| match event {
                TaskEvent::SpeedSectionDone { time_s } => Some(*time_s),
                _ => None,
            })
            .unwrap();
        assert!(kinds.contains(&TaskEvent::Reached { index: 4 }));
        assert!(kinds.contains(&TaskEvent::Reached { index: 5 }));
        assert_eq!(kinds.last(), Some(&TaskEvent::Goal));

        let end = flight.status(here(clock, goal));
        assert_eq!(end.phase, Phase::InGoal);
        assert!(end.next.is_none());
        assert_eq!(end.to_goal_m, 0.0);
        assert_eq!(end.speed_section_part, Some(1.0));
        assert_eq!(end.speed_section_time_s, Some(done));
        let length = flight.measured().speed_section_m.unwrap();
        assert_eq!(end.speed_section_average_ms, Some(length / done as f64));
        // The time runs from the gate, not from the crossing.
        assert_eq!(end.reached_unix_s[4].unwrap() - at(11, 0, 0), done);
        assert!(end.goal_unix_s.unwrap() > end.reached_unix_s[4].unwrap());
        assert_eq!(flight.next_index(), None);
    }

    #[test]
    fn a_crossing_before_the_gate_is_no_start_and_the_next_one_is() {
        let task = race();
        let launch = task.turnpoints[0].position;
        let mut flight = TaskFlight::new(task, at(9, 30, 0));
        let earth = Earth::Wgs84;
        let outside = earth.destination(launch, 270.0, 5_300.0);
        let inside = earth.destination(launch, 270.0, 4_700.0);

        // Out at 10:40: too early.
        let mut clock = at(10, 40, 0);
        let events = fly(&mut flight, &mut clock, inside, outside);
        assert!(events.is_empty());
        assert_eq!(
            flight.status(here(clock, outside)).phase,
            Phase::BeforeStart
        );
        // The aim is now back at the edge, from outside: 300 m away, and
        // a little more to the point from where the way west is shortest.
        let next = flight.status(here(clock, outside)).next.unwrap();
        assert!(!next.inside);
        assert!(
            next.distance_m >= 300.0 && next.distance_m < 305.0,
            "{}",
            next.distance_m
        );

        // A step that straddles the gate does not count either: its
        // first fix is from before it.
        flight.update(here(
            at(10, 59, 59),
            earth.destination(launch, 270.0, 5_010.0),
        ));
        let events = flight.update(here(
            at(11, 0, 0),
            earth.destination(launch, 270.0, 4_990.0),
        ));
        assert!(events.is_empty());

        // Out again after the gate: started. And in again, and out: the
        // last crossing is the start, and in a race the clock still runs
        // from the gate.
        let mut clock = at(11, 2, 0);
        let events = fly(&mut flight, &mut clock, inside, outside);
        assert_eq!(events.len(), 1);
        let events = fly(&mut flight, &mut clock, outside, inside);
        assert!(
            matches!(events[..], [(_, TaskEvent::Started { clock_from_unix_s })] if clock_from_unix_s == at(11, 0, 0))
        );
        let status = flight.status(here(clock, inside));
        assert_eq!(status.phase, Phase::OnTask);
        // Until the next turnpoint is reached the start is still shown.
        assert!(status.start.is_some());
        assert!(status.reached_unix_s[1].unwrap() > at(11, 2, 0));
    }

    #[test]
    fn with_several_gates_the_clock_runs_from_the_last_one_before_the_crossing() {
        let mut task = race();
        task.start = Some(Start {
            kind: StartKind::Race,
            gates: vec![
                TimeOfDay::new(11, 0, 0).unwrap(),
                TimeOfDay::new(11, 20, 0).unwrap(),
                TimeOfDay::new(11, 40, 0).unwrap(),
            ],
            close: None,
        });
        let launch = task.turnpoints[0].position;
        let mut flight = TaskFlight::new(task, at(9, 30, 0));
        let earth = Earth::Wgs84;
        let (inside, outside) = (
            earth.destination(launch, 270.0, 4_900.0),
            earth.destination(launch, 270.0, 5_100.0),
        );

        let waiting = flight.status(here(at(11, 5, 0), inside)).start.unwrap();
        assert_eq!(waiting.gate_unix_s, Some(at(11, 0, 0)));
        assert_eq!(waiting.opens_in_s, Some(-300));
        assert_eq!(waiting.next_gate_unix_s, Some(at(11, 20, 0)));
        assert_eq!(waiting.speed_to_gate_ms, None);

        let mut clock = at(11, 25, 0);
        let events = fly(&mut flight, &mut clock, inside, outside);
        assert!(
            matches!(events[..], [(_, TaskEvent::Started { clock_from_unix_s })] if clock_from_unix_s == at(11, 20, 0))
        );
    }

    #[test]
    fn in_an_elapsed_time_task_the_clock_runs_from_the_crossing() {
        let mut task = race();
        task.start = Some(Start {
            kind: StartKind::ElapsedTime,
            gates: vec![TimeOfDay::new(11, 0, 0).unwrap()],
            close: None,
        });
        let launch = task.turnpoints[0].position;
        let mut flight = TaskFlight::new(task, at(9, 30, 0));
        let earth = Earth::Wgs84;
        let mut clock = at(11, 31, 0);
        let events = fly(
            &mut flight,
            &mut clock,
            earth.destination(launch, 270.0, 4_900.0),
            earth.destination(launch, 270.0, 5_100.0),
        );
        let [(crossed, TaskEvent::Started { clock_from_unix_s })] = events[..] else {
            panic!("{events:?}");
        };
        assert_eq!(clock_from_unix_s, crossed);
        assert!(crossed > at(11, 31, 0));
    }

    #[test]
    fn a_goal_line_is_reached_across_it_or_in_the_half_circle_behind_it() {
        let earth = Earth::Wgs84;
        let goal = point(47.0, 11.5);
        let task = Task {
            kind: TaskKind::Competition,
            earth,
            turnpoints: vec![
                turnpoint("A", 47.0, 11.0, 400.0, Some(Role::Takeoff)),
                turnpoint("G", 47.0, 11.5, 200.0, None),
            ],
            takeoff_open: None,
            takeoff_close: None,
            start: None,
            goal: Goal {
                kind: GoalKind::Line,
                deadline: None,
                finish_altitude_m: None,
            },
        };
        // The way in at the goal, from the launch in the west.
        let inbound = earth.course(goal, point(47.0, 11.0)).bearing_deg + 180.0;
        let place = |past: f64, along: f64| {
            let stepped = earth.destination(goal, inbound, past);
            earth.destination(stepped, inbound + 90.0, along)
        };

        // Across the line near its end.
        let mut flight = TaskFlight::new(task.clone(), MIDNIGHT);
        assert!(flight.update(here(0, place(-20.0, 150.0))).is_empty());
        assert_eq!(
            flight.update(here(1, place(10.0, 150.0))),
            vec![TaskEvent::Reached { index: 1 }, TaskEvent::Goal]
        );

        // Past the end of the line: not in goal.
        let mut flight = TaskFlight::new(task.clone(), MIDNIGHT);
        flight.update(here(0, place(-20.0, 250.0)));
        assert!(flight.update(here(1, place(10.0, 250.0))).is_empty());
        // Then round into the half circle behind it: in goal.
        assert_eq!(
            flight.update(here(2, place(100.0, 100.0))).last(),
            Some(&TaskEvent::Goal)
        );

        // Behind the line but beyond the half circle: not in goal.
        let mut flight = TaskFlight::new(task, MIDNIGHT);
        flight.update(here(0, place(-20.0, 250.0)));
        assert!(flight.update(here(1, place(180.0, 150.0))).is_empty());
        let status = flight.status(here(1, place(180.0, 150.0)));
        assert_eq!(status.phase, Phase::OnTask);
        // A task without a start has no speed section to report.
        assert_eq!(status.speed_section_part, None);
        assert!(status.start.is_none());
    }

    #[test]
    fn a_plain_route_is_flown_from_waypoint_to_waypoint() {
        let earth = Earth::Wgs84;
        let task = Task {
            kind: TaskKind::Route,
            earth,
            turnpoints: vec![
                turnpoint("A", 47.0, 11.0, 0.0, None),
                turnpoint("B", 47.1, 11.2, 0.0, None),
                turnpoint("C", 47.0, 11.4, 0.0, None),
            ],
            takeoff_open: None,
            takeoff_close: None,
            start: None,
            goal: Goal::default(),
        }
        .checked()
        .unwrap();
        let (a, b, c) = (
            task.turnpoints[0].position,
            task.turnpoints[1].position,
            task.turnpoints[2].position,
        );
        let mut flight = TaskFlight::new(task, MIDNIGHT);
        let status = flight.status(here(MIDNIGHT, a));
        assert_eq!(status.phase, Phase::OnTask);
        assert_eq!(status.next.as_ref().unwrap().aim, b);
        assert!((status.to_goal_m - earth.distance_m(a, b) - earth.distance_m(b, c)).abs() < 1e-6);
        assert_eq!(status.way, vec![a, b, c]);

        // Passing B at 300 m counts; the route's waypoints have a reach
        // of 400 m.
        let near_b = earth.destination(b, 0.0, 300.0);
        let mut clock = MIDNIGHT;
        let events = fly(&mut flight, &mut clock, a, near_b);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].1, TaskEvent::Reached { index: 1 });
        let events = fly(&mut flight, &mut clock, near_b, c);
        assert_eq!(events.last().unwrap().1, TaskEvent::Goal);
    }

    #[test]
    fn nothing_counts_after_the_deadline() {
        let task = race();
        let launch = task.turnpoints[0].position;
        let mut flight = TaskFlight::new(task, at(9, 30, 0));
        let earth = Earth::Wgs84;
        let mut clock = at(16, 0, 1);
        let events = fly(
            &mut flight,
            &mut clock,
            earth.destination(launch, 270.0, 4_900.0),
            earth.destination(launch, 270.0, 5_100.0),
        );
        assert!(events.is_empty());
    }

    #[test]
    fn the_pilot_can_skip_a_turnpoint_and_step_back() {
        let task = race();
        let launch = task.turnpoints[0].position;
        let mut flight = TaskFlight::new(task, at(9, 30, 0));
        let earth = Earth::Wgs84;
        let mut clock = at(11, 5, 0);
        fly(
            &mut flight,
            &mut clock,
            earth.destination(launch, 270.0, 4_900.0),
            earth.destination(launch, 270.0, 5_100.0),
        );
        assert_eq!(flight.next_index(), Some(2));
        flight.skip();
        assert_eq!(flight.next_index(), Some(3));
        // Back to WEST, and back again to the start: the clock is
        // forgotten with it.
        flight.step_back();
        assert_eq!(flight.next_index(), Some(2));
        assert!(
            flight
                .status(here(clock, launch))
                .clock_from_unix_s
                .is_some()
        );
        flight.step_back();
        assert_eq!(flight.next_index(), Some(1));
        let status = flight.status(here(clock, launch));
        assert_eq!(status.phase, Phase::BeforeStart);
        assert_eq!(status.clock_from_unix_s, None);
        assert_eq!(status.reached_unix_s[1], None);
        // Not further back than the first turnpoint to reach.
        flight.step_back();
        assert_eq!(flight.next_index(), Some(1));
    }
}
