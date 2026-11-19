//! A task: a launch, a row of turnpoints to round in order, and a goal.
//!
//! This is the task of a competition day, with its cylinders, its start
//! and its goal, and also the plain route of waypoints a pilot plans a
//! cross-country flight along. The rules it follows are those of the FAI
//! Sporting Code, Section 7F (2024 edition), which CIVL's competitions
//! are scored by:
//!
//! - The first turnpoint is the launch. Distances are measured from its
//!   centre, whatever radius it was given, and it is not a place to reach
//!   (6.4, annex A 2.2) unless it is also the start.
//! - Every other turnpoint is a cylinder, reached by crossing its edge.
//!   The direction of the crossing does not matter (6.2.1).
//! - A goal is a cylinder, or a line with a half circle behind it
//!   (6.2.3.1).
//! - Distances are measured on the WGS84 ellipsoid (4.2), unless the task
//!   itself asks for the FAI sphere, which the task format allows.
//!
//! The format tasks travel in is XCTrack's, read and written by
//! [`xctsk`]. The shortest way through a task is found by [`route`], and
//! [`progress`] follows a flight through it.

pub mod progress;
pub mod route;
pub mod xctsk;

use std::fmt;

use venti_domain::GeoPoint;

use crate::geodesy::Earth;

/// A task holds no more turnpoints than this. The longest competition
/// tasks have fewer than twenty.
pub const MAX_TURNPOINTS: usize = 100;

/// No cylinder is wider than this, metres.
pub const MAX_RADIUS_M: f64 = 500_000.0;

/// How near a pilot has to come to a waypoint of a plain route for it to
/// count as rounded, metres. A route's waypoints have no cylinder of
/// their own. 400 m is the customary radius of a turnpoint; the choice is
/// ours.
pub const ROUTE_POINT_REACH_M: f64 = 400.0;

/// A time of day in UTC, as tasks give their times: a task is set for
/// "today", whichever day it is flown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeOfDay(u32);

const DAY_S: i64 = 86_400;

impl TimeOfDay {
    /// A time from its hour, minute and second. `None` when one of them
    /// is out of range.
    pub fn new(hour: u32, minute: u32, second: u32) -> Option<Self> {
        (hour < 24 && minute < 60 && second < 60).then(|| Self(hour * 3600 + minute * 60 + second))
    }

    /// Seconds since midnight UTC.
    pub fn seconds(self) -> u32 {
        self.0
    }

    /// Reads "12:30:00Z", the form the task format uses. The seconds and
    /// the closing letter may be missing.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_suffix(['Z', 'z']).unwrap_or(text);
        let mut parts = text.split(':');
        let hour = parts.next()?.parse().ok()?;
        let minute = parts.next()?.parse().ok()?;
        let second = match parts.next() {
            Some(second) => second.parse().ok()?,
            None => 0,
        };
        if parts.next().is_some() {
            return None;
        }
        Self::new(hour, minute, second)
    }

    /// The moment this time of day means for a flight: the one nearest to
    /// a moment of that flight, seconds since 1970-01-01 UTC. A task
    /// flown far from Greenwich may open before midnight UTC and close
    /// after it; taking the nearest occurrence keeps its times in order
    /// as long as none lies more than twelve hours from the anchor.
    pub fn near(self, anchor_unix_s: i64) -> i64 {
        let midnight = anchor_unix_s.div_euclid(DAY_S) * DAY_S;
        let same_day = midnight + i64::from(self.0);
        if same_day - anchor_unix_s > DAY_S / 2 {
            same_day - DAY_S
        } else if anchor_unix_s - same_day > DAY_S / 2 {
            same_day + DAY_S
        } else {
            same_day
        }
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (hour, minute, second) = (self.0 / 3600, self.0 / 60 % 60, self.0 % 60);
        write!(f, "{hour:02}:{minute:02}:{second:02}Z")
    }
}

/// What a turnpoint is in its task, beyond a place to round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The launch, marked as such. Only the first turnpoint can be.
    Takeoff,
    /// The start of the speed section: the clock starts at its edge.
    Start,
    /// The end of the speed section: the clock stops at its edge.
    EndOfSpeed,
}

/// One turnpoint of a task.
#[derive(Debug, Clone, PartialEq)]
pub struct Turnpoint {
    /// Its name, as in the waypoint file of the competition.
    pub name: String,
    /// What the waypoint file says about it.
    pub description: Option<String>,
    /// Its centre.
    pub position: GeoPoint,
    /// The height of the ground there, metres above sea level.
    pub altitude_m: f64,
    /// The radius of its cylinder, metres. For a goal line, half the
    /// line's length. Zero for a waypoint of a plain route.
    pub radius_m: f64,
    /// Its part in the task.
    pub role: Option<Role>,
}

/// How the clock starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKind {
    /// Race to goal: everyone's clock starts when a gate opens.
    Race,
    /// Elapsed time: a pilot's clock starts when the pilot crosses.
    ElapsedTime,
}

/// The start of the speed section.
#[derive(Debug, Clone, PartialEq)]
pub struct Start {
    /// Whose clock starts when.
    pub kind: StartKind,
    /// When the gates open, the earliest first. Usually one.
    pub gates: Vec<TimeOfDay>,
    /// When the start closes.
    pub close: Option<TimeOfDay>,
}

/// The shape of a goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GoalKind {
    /// The cylinder of the last turnpoint.
    #[default]
    Cylinder,
    /// A line through the last turnpoint, across the way in, as long as
    /// twice its radius.
    Line,
}

/// The goal, and when the task ends.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Goal {
    /// Its shape.
    pub kind: GoalKind,
    /// When the task ends. A goal reached later does not count.
    pub deadline: Option<TimeOfDay>,
    /// A height above the goal's ground to arrive over, metres. Arriving
    /// lower costs points in some competitions.
    pub finish_altitude_m: Option<f64>,
}

/// The kind of task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// A competition task: cylinders, a start and a goal.
    Competition,
    /// A plain route from waypoint to waypoint, without cylinders.
    Route,
}

/// A task.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    /// What kind of task it is.
    pub kind: TaskKind,
    /// The shape of the earth its cylinders are measured on.
    pub earth: Earth,
    /// The launch, the turnpoints in the order to round them, and the
    /// goal last.
    pub turnpoints: Vec<Turnpoint>,
    /// When the launch opens and closes.
    pub takeoff_open: Option<TimeOfDay>,
    /// When the launch closes.
    pub takeoff_close: Option<TimeOfDay>,
    /// How the clock starts, if the task says.
    pub start: Option<Start>,
    /// The goal.
    pub goal: Goal,
}

/// Why something is not a task.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TaskError {
    /// The text is not in the task format at all.
    #[error("this is not a task in the XCTSK format")]
    NotATask,
    /// It is a task, of a kind or version this app does not know.
    #[error("this task is of a kind this app does not know: {0}")]
    Unknown(String),
    /// The compressed form of a QR code could not be unpacked.
    #[error("the compressed task could not be unpacked")]
    Compressed,
    /// The text is longer than any task.
    #[error("this is too long to be a task")]
    TooLong,
    /// A task needs a launch and a goal at least.
    #[error("a task needs at least two turnpoints, and this has {0}")]
    TooFewTurnpoints(usize),
    /// More turnpoints than any task has.
    #[error("a task has at most {MAX_TURNPOINTS} turnpoints, and this has {0}")]
    TooManyTurnpoints(usize),
    /// One turnpoint cannot be read. Turnpoints are counted from 1.
    #[error("turnpoint {number}: {problem}")]
    Turnpoint {
        /// Which one, counted from 1.
        number: usize,
        /// What is wrong with it.
        problem: String,
    },
    /// A time is not a time of day.
    #[error("\"{0}\" is not a time of day")]
    Time(String),
    /// The start and the end of the speed section do not fit together.
    #[error("{0}")]
    SpeedSection(&'static str),
}

impl Task {
    /// Checks that the parts of a task fit together, and returns it.
    pub fn checked(self) -> Result<Self, TaskError> {
        let count = self.turnpoints.len();
        if count < 2 {
            return Err(TaskError::TooFewTurnpoints(count));
        }
        if count > MAX_TURNPOINTS {
            return Err(TaskError::TooManyTurnpoints(count));
        }
        for (index, turnpoint) in self.turnpoints.iter().enumerate() {
            let problem = |problem: &str| TaskError::Turnpoint {
                number: index + 1,
                problem: problem.to_owned(),
            };
            if !turnpoint.radius_m.is_finite() || turnpoint.radius_m < 0.0 {
                return Err(problem("its radius is not a length"));
            }
            if turnpoint.radius_m > MAX_RADIUS_M {
                return Err(problem("its radius is wider than any cylinder"));
            }
            if !turnpoint.altitude_m.is_finite() {
                return Err(problem("its height is not a number"));
            }
            if turnpoint.role == Some(Role::Takeoff) && index != 0 {
                return Err(problem("only the first turnpoint can be the take-off"));
            }
            // A cylinder without a width cannot be crossed. The launch
            // needs none, and a route's waypoints have none by design.
            if self.kind == TaskKind::Competition && index > 0 && turnpoint.radius_m == 0.0 {
                return Err(problem("its cylinder has no radius"));
            }
        }
        let with_role = |role: Role| {
            self.turnpoints
                .iter()
                .enumerate()
                .filter(move |(_, turnpoint)| turnpoint.role == Some(role))
                .map(|(index, _)| index)
                .collect::<Vec<_>>()
        };
        let (starts, ends) = (with_role(Role::Start), with_role(Role::EndOfSpeed));
        if starts.len() > 1 {
            return Err(TaskError::SpeedSection(
                "more than one turnpoint is marked as the start of the speed section",
            ));
        }
        if ends.len() > 1 {
            return Err(TaskError::SpeedSection(
                "more than one turnpoint is marked as the end of the speed section",
            ));
        }
        if let (Some(start), Some(end)) = (starts.first(), ends.first())
            && end <= start
        {
            return Err(TaskError::SpeedSection(
                "the end of the speed section comes before its start",
            ));
        }
        if starts.is_empty() && !ends.is_empty() {
            return Err(TaskError::SpeedSection(
                "the speed section has an end and no start",
            ));
        }
        Ok(self)
    }

    /// The first turnpoint that has to be reached. The launch is not one,
    /// unless the clock starts at it.
    pub fn first_to_reach(&self) -> usize {
        match self.turnpoints.first().and_then(|first| first.role) {
            Some(Role::Start | Role::EndOfSpeed) => 0,
            _ => 1,
        }
    }

    /// The turnpoint the clock starts at, if the task has a speed section.
    pub fn start_index(&self) -> Option<usize> {
        self.turnpoints
            .iter()
            .position(|turnpoint| turnpoint.role == Some(Role::Start))
    }

    /// The turnpoint the clock stops at: the one marked so, or the goal.
    /// `None` when the task has no speed section.
    pub fn end_of_speed_index(&self) -> Option<usize> {
        self.start_index()?;
        Some(
            self.turnpoints
                .iter()
                .position(|turnpoint| turnpoint.role == Some(Role::EndOfSpeed))
                .unwrap_or(self.goal_index()),
        )
    }

    /// The goal: the last turnpoint.
    pub fn goal_index(&self) -> usize {
        self.turnpoints.len().saturating_sub(1)
    }

    /// The area to reach at a turnpoint.
    pub fn zone(&self, index: usize) -> Option<route::Zone> {
        let turnpoint = self.turnpoints.get(index)?;
        if turnpoint.radius_m == 0.0 {
            return Some(route::Zone::Point {
                centre: turnpoint.position,
                reach_m: ROUTE_POINT_REACH_M,
            });
        }
        if index == self.goal_index() && self.goal.kind == GoalKind::Line {
            // The line lies across the way in from the last turnpoint
            // that is somewhere else (Section 7F, 6.2.3.1). Without one
            // there is no way in, and the goal is its cylinder.
            let before = self.turnpoints[..index]
                .iter()
                .rev()
                .find(|earlier| self.earth.distance_m(earlier.position, turnpoint.position) > 1.0);
            if let Some(before) = before {
                return Some(route::Zone::Line {
                    centre: turnpoint.position,
                    half_length_m: turnpoint.radius_m,
                    inbound_deg: self
                        .earth
                        .course(turnpoint.position, before.position)
                        .bearing_deg
                        + 180.0,
                });
            }
        }
        Some(route::Zone::Cylinder {
            centre: turnpoint.position,
            radius_m: turnpoint.radius_m,
        })
    }

    /// The areas to reach from a turnpoint on, in order, each with the
    /// index of its turnpoint.
    pub fn zones_from(&self, index: usize) -> Vec<(usize, route::Zone)> {
        (index..self.turnpoints.len())
            .filter_map(|index| Some((index, self.zone(index)?)))
            .collect()
    }

    /// The shortest way through the task from the launch, and what it
    /// measures.
    pub fn measure(&self) -> Measured {
        let Some(launch) = self.turnpoints.first() else {
            return Measured::default();
        };
        let first = self.first_to_reach();
        let zones: Vec<route::Zone> = self
            .zones_from(first)
            .into_iter()
            .map(|(_, zone)| zone)
            .collect();
        let found = route::shortest(self.earth, launch.position, &zones);

        // One point for every turnpoint: the launch's centre, then where
        // the shortest way touches each.
        let mut points = Vec::with_capacity(self.turnpoints.len());
        if first == 1 {
            points.push(launch.position);
        }
        points.extend(found.points.iter().copied());
        let mut along = Vec::with_capacity(points.len());
        let mut sum = 0.0;
        for (index, point) in points.iter().enumerate() {
            if index > 0 {
                sum += self.earth.distance_m(points[index - 1], *point);
            }
            along.push(sum);
        }
        let speed_section_m = self
            .start_index()
            .zip(self.end_of_speed_index())
            .and_then(|(start, end)| Some(along.get(end)? - along.get(start)?));
        Measured {
            points,
            along_m: along,
            length_m: sum,
            speed_section_m,
        }
    }
}

/// A task measured along its shortest way.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Measured {
    /// Where the shortest way touches each turnpoint, in their order; the
    /// launch's centre first.
    pub points: Vec<GeoPoint>,
    /// How far along the way each of those points lies, metres.
    pub along_m: Vec<f64>,
    /// The length of the task, metres: from the launch's centre through
    /// every turnpoint to the goal.
    pub length_m: f64,
    /// The length of the speed section, metres, if the task has one.
    pub speed_section_m: Option<f64>,
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    pub fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    pub fn turnpoint(
        name: &str,
        lat: f64,
        lon: f64,
        radius_m: f64,
        role: Option<Role>,
    ) -> Turnpoint {
        Turnpoint {
            name: name.to_owned(),
            description: None,
            position: point(lat, lon),
            altitude_m: 800.0,
            radius_m,
            role,
        }
    }

    /// A task of a day in the Inn valley: launch, a start cylinder around
    /// the launch to leave, two turnpoints, and a goal line 400 m wide
    /// with the end of the speed section two kilometres around it.
    pub fn race() -> Task {
        Task {
            kind: TaskKind::Competition,
            earth: Earth::Wgs84,
            turnpoints: vec![
                turnpoint("LAUNCH", 47.30, 11.40, 400.0, Some(Role::Takeoff)),
                turnpoint("START", 47.30, 11.40, 5_000.0, Some(Role::Start)),
                turnpoint("WEST", 47.28, 11.10, 2_000.0, None),
                turnpoint("EAST", 47.35, 11.70, 4_000.0, None),
                turnpoint("GOAL", 47.26, 11.45, 2_000.0, Some(Role::EndOfSpeed)),
                turnpoint("GOAL", 47.26, 11.45, 200.0, None),
            ],
            takeoff_open: TimeOfDay::new(9, 0, 0),
            takeoff_close: TimeOfDay::new(12, 0, 0),
            start: Some(Start {
                kind: StartKind::Race,
                gates: vec![TimeOfDay::new(11, 0, 0).unwrap()],
                close: None,
            }),
            goal: Goal {
                kind: GoalKind::Line,
                deadline: TimeOfDay::new(16, 0, 0),
                finish_altitude_m: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn a_time_of_day_is_read_and_written_the_way_tasks_give_it() {
        let time = TimeOfDay::parse("12:30:05Z").unwrap();
        assert_eq!(time.seconds(), 12 * 3600 + 30 * 60 + 5);
        assert_eq!(time.to_string(), "12:30:05Z");
        assert_eq!(TimeOfDay::parse("09:00"), TimeOfDay::new(9, 0, 0));
        assert_eq!(TimeOfDay::parse(" 9:00:00z "), TimeOfDay::new(9, 0, 0));
        for bad in ["", "noon", "24:00:00Z", "12:60:00Z", "12:00:00:00Z", "12"] {
            assert_eq!(TimeOfDay::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_time_of_day_means_its_occurrence_nearest_to_the_flight() {
        // 4 July 2026, 10:00 UTC.
        let anchor = 1_783_159_200;
        let midnight = anchor - 10 * 3600;
        assert_eq!(
            TimeOfDay::new(13, 0, 0).unwrap().near(anchor),
            midnight + 13 * 3600
        );
        // A flight in Australia: launched at 23:00 UTC, the gate at 01:30
        // is the next UTC day's, the launch window from 22:00 this day's.
        let late = midnight + 23 * 3600;
        assert_eq!(
            TimeOfDay::new(1, 30, 0).unwrap().near(late),
            midnight + DAY_S + 3600 + 1800
        );
        assert_eq!(
            TimeOfDay::new(22, 0, 0).unwrap().near(late),
            midnight + 22 * 3600
        );
        // And just after midnight UTC, the window that opened before it.
        let early = midnight + 600;
        assert_eq!(
            TimeOfDay::new(22, 0, 0).unwrap().near(early),
            midnight - 2 * 3600
        );
    }

    #[test]
    fn the_parts_of_a_task_are_found_by_their_marks() {
        let task = race().checked().unwrap();
        assert_eq!(task.first_to_reach(), 1);
        assert_eq!(task.start_index(), Some(1));
        assert_eq!(task.end_of_speed_index(), Some(4));
        assert_eq!(task.goal_index(), 5);

        // Without a mark for the end, the clock stops at the goal.
        let mut plain = race();
        plain.turnpoints[4].role = None;
        assert_eq!(plain.end_of_speed_index(), Some(5));
        // Without a start there is no speed section.
        plain.turnpoints[1].role = None;
        assert_eq!(plain.end_of_speed_index(), None);
        // A task whose first turnpoint is the start has it to reach.
        let mut starts_first = race();
        starts_first.turnpoints.remove(0);
        assert_eq!(starts_first.first_to_reach(), 0);
    }

    #[test]
    fn a_goal_line_lies_across_the_way_in_from_the_last_place_that_is_elsewhere() {
        let task = race();
        // The turnpoint before the goal has the goal's centre, so the way
        // in is from EAST, to the north-east of the goal.
        let Some(route::Zone::Line {
            half_length_m,
            inbound_deg,
            ..
        }) = task.zone(5)
        else {
            panic!("the goal is not a line");
        };
        assert_eq!(half_length_m, 200.0);
        let from_east = task
            .earth
            .course(task.turnpoints[3].position, task.turnpoints[5].position)
            .bearing_deg;
        // The direction at the goal differs a little from the one at the
        // far end of the leg: meridians converge.
        assert!(
            (inbound_deg.rem_euclid(360.0) - from_east).abs() < 0.5,
            "{inbound_deg} {from_east}"
        );
        // The end of the speed section with the same centre is a cylinder.
        assert!(
            matches!(task.zone(4), Some(route::Zone::Cylinder { radius_m, .. }) if radius_m == 2_000.0)
        );
    }

    #[test]
    fn a_task_that_does_not_fit_together_is_refused_with_the_reason() {
        let mut short = race();
        short.turnpoints.truncate(1);
        assert_eq!(short.checked(), Err(TaskError::TooFewTurnpoints(1)));

        let mut backwards = race();
        backwards.turnpoints.swap(1, 4);
        assert!(matches!(
            backwards.checked(),
            Err(TaskError::SpeedSection(_))
        ));

        let mut twice = race();
        twice.turnpoints[2].role = Some(Role::Start);
        assert!(matches!(twice.checked(), Err(TaskError::SpeedSection(_))));

        let mut flat = race();
        flat.turnpoints[2].radius_m = 0.0;
        let refused = flat.checked().unwrap_err();
        assert_eq!(
            refused.to_string(),
            "turnpoint 3: its cylinder has no radius"
        );

        let mut late_launch = race();
        late_launch.turnpoints[3].role = Some(Role::Takeoff);
        assert!(matches!(
            late_launch.checked(),
            Err(TaskError::Turnpoint { number: 4, .. })
        ));
    }

    #[test]
    fn a_task_is_measured_from_the_launch_through_the_edges_of_its_cylinders() {
        let task = race();
        let measured = task.measure();
        assert_eq!(measured.points.len(), task.turnpoints.len());
        assert_eq!(measured.points[0], task.turnpoints[0].position);
        assert_eq!(measured.along_m[0], 0.0);

        // Through the centres the task is far longer than along the edges.
        let through_centres: f64 = task
            .turnpoints
            .windows(2)
            .map(|pair| task.earth.distance_m(pair[0].position, pair[1].position))
            .sum();
        assert!(measured.length_m < through_centres - 8_000.0);
        // West to within 2 km of WEST (about 22.8 km from the launch),
        // back east to 4 km before EAST (46 km from WEST) and down to
        // the goal (21 km from EAST): about 78 km.
        assert!(
            measured.length_m > 74_000.0 && measured.length_m < 82_000.0,
            "{}",
            measured.length_m
        );
        // Each point lies on the edge of its cylinder.
        for index in 1..5 {
            let from_centre = task
                .earth
                .distance_m(task.turnpoints[index].position, measured.points[index]);
            assert!(
                (from_centre - task.turnpoints[index].radius_m).abs() < 0.01,
                "turnpoint {index}: {from_centre}"
            );
        }
        // The speed section runs from the start's edge to the edge of
        // the cylinder around the goal.
        let speed = measured.speed_section_m.unwrap();
        assert!((speed - (measured.along_m[4] - measured.along_m[1])).abs() < 1e-6);
        assert!(speed < measured.length_m - 5_000.0);
    }
}
