//! Whether what the take-off rule took for a flight is one.
//!
//! The take-off rule is quick on purpose: a pilot wants the vario and the
//! recording from the first seconds. A cable car, a chairlift and a car on
//! a mountain pass meet it as well. So a flight begins provisional, and it
//! is a flight once it has done something that a glider does and a lift or
//! a car does not ([`Evidence`]). One that has not, and moves as fast as a
//! car on a road or comes to rest for good, is taken back ([`FalseStart`]).
//!
//! The rules lean one way. A car that passes for a glider costs a line in
//! the logbook that the pilot deletes; a flight taken back costs the
//! flight. So what confirms a flight is generous, and what takes one back
//! is something no glider does: only a flight that has shown none of the
//! four signs below in all its time in the air is ever taken back, and
//! only once it has stood still for good or driven at motorway speed.
//!
//! All the figures are ours, to verify against flights, lift rides and
//! drives that were recorded.

use std::collections::VecDeque;

use super::computer::TrailPoint;

/// What showed a flight to be one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// A full circle one way round: the course over the ground turned
    /// [`CIRCLE_DEG`] within [`CIRCLE_S`], or a circle gave a wind. Roads
    /// turn back and forth; a loop of a motorway junction is three
    /// quarters of a circle.
    Circles,
    /// [`HIGH_M`] or more over the ground for [`HIGH_S`], not climbing
    /// faster than [`HIGH_CLIMB_MS`]: a car is on the ground, and a cable
    /// car that hangs as high climbs faster. Only where the ground's height
    /// is known.
    HighOverGround,
    /// A glide: for [`GLIDE_S`] losing height as a glider does, along a
    /// line, at a steady speed and a glide over the ground a glider makes
    /// ([`GLIDE_SINK_MS`], [`GLIDE_RATIO`], [`GLIDE_STRAIGHT`],
    /// [`GLIDE_STEADY_MS`]). A lift climbs; a road down a mountain winds,
    /// and a car on it slows for the bends.
    Glide,
    /// Soaring a ridge: [`SOARING_REVERSALS`] turns back within
    /// [`SOARING_S`] at one height, within [`SOARING_RANGE_M`]. A road
    /// that turns back on itself climbs or falls as it does.
    Soaring,
}

/// Why a flight that had not shown itself to be one was taken back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalseStart {
    /// It moved at [`VEHICLE_MS`] or more for [`VEHICLE_S`]: a car on an
    /// open road, a train.
    Vehicle,
    /// It came to rest, and stayed at rest for as long as a landing takes
    /// to stand, without having done anything only a glider does: a cable
    /// car at its top station, a chairlift, a car parked at a pass.
    NothingFlown,
}

/// See [`Evidence::Circles`]: degrees.
pub const CIRCLE_DEG: f64 = 360.0;
/// See [`Evidence::Circles`]: seconds.
pub const CIRCLE_S: f64 = 60.0;
/// See [`Evidence::HighOverGround`]: metres over the ground.
pub const HIGH_M: f64 = 100.0;
/// See [`Evidence::HighOverGround`]: seconds.
pub const HIGH_S: f64 = 60.0;
/// See [`Evidence::HighOverGround`]: metres per second, over those
/// seconds.
pub const HIGH_CLIMB_MS: f64 = 1.0;
/// See [`Evidence::Glide`]: seconds of trail looked back on.
pub const GLIDE_S: f64 = 60.0;
/// See [`Evidence::Glide`]: the height lost a second on average, metres.
/// A paraglider's own sink is about one metre a second; three and a half
/// is a glider with its ears in.
pub const GLIDE_SINK_MS: std::ops::RangeInclusive<f64> = 0.5..=3.5;
/// See [`Evidence::Glide`]: metres over the ground for each metre of
/// height lost. A road of 5 % is a glide of twenty.
pub const GLIDE_RATIO: std::ops::RangeInclusive<f64> = 3.0..=15.0;
/// See [`Evidence::Glide`]: the straight line from the first place to the
/// last is at least this share of the way flown. Weaving 30 degrees either
/// side of the line is 0.93; a road that bends 50 degrees either way, 0.8.
pub const GLIDE_STRAIGHT: f64 = 0.9;
/// See [`Evidence::Glide`]: the speed over the ground of each ten seconds
/// of the minute lies within this many metres per second of the others'.
/// A glider pushing its speed bar half way goes 3 m/s faster.
pub const GLIDE_STEADY_MS: f64 = 5.0;
/// See [`Evidence::Glide`]: metres per second over the ground at least,
/// on average.
pub const GLIDE_SPEED_MS: f64 = 4.0;
/// See [`Evidence::Soaring`]: how many turns back.
pub const SOARING_REVERSALS: usize = 3;
/// See [`Evidence::Soaring`]: seconds.
pub const SOARING_S: f64 = 300.0;
/// See [`Evidence::Soaring`]: metres from the lowest to the highest.
pub const SOARING_RANGE_M: f64 = 150.0;
/// A turn back: the course turned this many degrees one way within
/// [`REVERSAL_S`] seconds.
const REVERSAL_DEG: f64 = 150.0;
/// See [`REVERSAL_DEG`].
const REVERSAL_S: f64 = 40.0;
/// Two turns back are at least this many seconds apart; a circle flown
/// slowly is no two.
const REVERSAL_APART_S: f64 = 20.0;
/// See [`FalseStart::Vehicle`]: metres per second, 108 km/h. A paraglider
/// on full bar with a strong wind behind it is seldom faster than 25.
pub const VEHICLE_MS: f64 = 30.0;
/// See [`FalseStart::Vehicle`]: seconds.
pub const VEHICLE_S: f64 = 30.0;
/// The course is counted only while the glider moves at least this fast
/// over the ground, metres per second: slower, a receiver's course
/// wanders by tens of degrees from one second to the next.
pub const COURSE_FROM_MS: f64 = 3.0;
/// A step of the course larger than this, degrees, says nothing of which
/// way round it went.
const COURSE_STEP_AT_MOST_DEG: f64 = 120.0;

/// What the provisional flight has shown so far.
#[derive(Debug, Clone, Default)]
pub(super) struct FlightCheck {
    /// The course turned since take-off, degrees, positive to the right,
    /// with when: as far back as the longest window looks.
    turns: VecDeque<(f64, f64)>,
    turned_deg: f64,
    last_course: Option<(f64, f64)>,
    /// When the turns back of the last [`SOARING_S`] were seen.
    reversals: VecDeque<f64>,
    /// The altitudes of the last [`SOARING_S`], with when.
    altitudes: VecDeque<(f64, f64)>,
    /// Since when, and from which altitude, the glider has been high over
    /// the ground.
    high_since: Option<(f64, f64)>,
    /// Since when it has moved as fast as a vehicle.
    fast_since: Option<f64>,
}

/// One moment of a provisional flight, as the check takes it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Moment {
    pub time_s: f64,
    pub altitude_m: Option<f64>,
    pub speed_ms: Option<f64>,
    pub course_deg: Option<f64>,
    pub above_ground_m: Option<f64>,
    /// Whether a circle just gave a wind.
    pub circled: bool,
}

impl FlightCheck {
    /// Takes a moment of a flight not yet confirmed: what it shows, if it
    /// shows that it is a flight, or that it is a vehicle.
    pub(super) fn add(
        &mut self,
        moment: Moment,
        trail: &VecDeque<TrailPoint>,
    ) -> Option<Result<Evidence, FalseStart>> {
        let now = moment.time_s;
        let looked_back_s = SOARING_S.max(CIRCLE_S).max(REVERSAL_S);

        // The course, turned through.
        match (moment.speed_ms, moment.course_deg) {
            (Some(speed), Some(course)) if speed >= COURSE_FROM_MS && course.is_finite() => {
                if let Some((then, last)) = self.last_course {
                    let step = (course - last + 540.0).rem_euclid(360.0) - 180.0;
                    if now - then <= 5.0 && step.abs() <= COURSE_STEP_AT_MOST_DEG {
                        self.turned_deg += step;
                    }
                }
                self.last_course = Some((now, course));
            }
            _ => self.last_course = None,
        }
        self.turns.push_back((now, self.turned_deg));
        while self
            .turns
            .front()
            .is_some_and(|&(time, _)| time < now - looked_back_s)
        {
            self.turns.pop_front();
        }
        // The most the course has turned one way to now, since a moment.
        let turned_since = |since_s: f64| {
            self.turns
                .iter()
                .filter(|&&(time, _)| time >= since_s)
                .map(|&(_, turned)| (self.turned_deg - turned).abs())
                .fold(0.0, f64::max)
        };

        if moment.circled || turned_since(now - CIRCLE_S) >= CIRCLE_DEG {
            return Some(Ok(Evidence::Circles));
        }

        // High over the ground, and not climbing as a cable car does.
        match (moment.above_ground_m, moment.altitude_m) {
            (Some(above), Some(altitude)) if above >= HIGH_M => {
                let (since, from) = *self.high_since.get_or_insert((now, altitude));
                let seconds = now - since;
                if seconds >= HIGH_S && (altitude - from) / seconds <= HIGH_CLIMB_MS {
                    return Some(Ok(Evidence::HighOverGround));
                }
            }
            _ => self.high_since = None,
        }

        if glides(trail, now) {
            return Some(Ok(Evidence::Glide));
        }

        // Soaring: turns back at one height. A turn back is counted from
        // the last one, so that one turn is not counted twice while it is
        // in the window.
        let last = self.reversals.back().copied().unwrap_or(f64::NEG_INFINITY);
        if turned_since((now - REVERSAL_S).max(last)) >= REVERSAL_DEG
            && now - last >= REVERSAL_APART_S
        {
            self.reversals.push_back(now);
        }
        while self
            .reversals
            .front()
            .is_some_and(|&time| time < now - SOARING_S)
        {
            self.reversals.pop_front();
        }
        if let Some(altitude) = moment.altitude_m {
            self.altitudes.push_back((now, altitude));
        }
        while self
            .altitudes
            .front()
            .is_some_and(|&(time, _)| time < now - SOARING_S)
        {
            self.altitudes.pop_front();
        }
        let (low, high) = self.altitudes.iter().fold(
            (f64::INFINITY, f64::NEG_INFINITY),
            |(low, high), &(_, altitude)| (low.min(altitude), high.max(altitude)),
        );
        if self.reversals.len() >= SOARING_REVERSALS && high - low <= SOARING_RANGE_M {
            return Some(Ok(Evidence::Soaring));
        }

        // As fast as a car on an open road.
        if moment.speed_ms.is_some_and(|speed| speed >= VEHICLE_MS) {
            let since = *self.fast_since.get_or_insert(now);
            if now - since >= VEHICLE_S {
                return Some(Err(FalseStart::Vehicle));
            }
        } else {
            self.fast_since = None;
        }
        None
    }
}

/// Whether the trail of the last [`GLIDE_S`] is a glide ([`Evidence::Glide`]).
fn glides(trail: &VecDeque<TrailPoint>, now: f64) -> bool {
    let Some(first) = trail.iter().position(|point| point.time_s >= now - GLIDE_S) else {
        return false;
    };
    let points: Vec<&TrailPoint> = trail.iter().skip(first).collect();
    let (Some(start), Some(end)) = (points.first(), points.last()) else {
        return false;
    };
    let seconds = end.time_s - start.time_s;
    // A minute with a second or two missing still counts.
    if seconds < 0.9 * GLIDE_S {
        return false;
    }
    let way: f64 = points
        .windows(2)
        .map(|pair| pair[0].position.distance_m(pair[1].position))
        .sum();
    let lost = start.altitude_m - end.altitude_m;
    let chord = start.position.distance_m(end.position);
    // The speed of each ten seconds, from where the glider was at their
    // ends: steadier than from one second to the next.
    let speeds: Vec<f64> = points
        .iter()
        .step_by(10)
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|pair| pair[1].time_s > pair[0].time_s)
        .map(|pair| {
            pair[0].position.distance_m(pair[1].position) / (pair[1].time_s - pair[0].time_s)
        })
        .collect();
    let (slowest, fastest) = speeds
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), &speed| {
            (low.min(speed), high.max(speed))
        });
    lost > 0.0
        && GLIDE_SINK_MS.contains(&(lost / seconds))
        && way / seconds >= GLIDE_SPEED_MS
        && chord >= GLIDE_STRAIGHT * way
        && GLIDE_RATIO.contains(&(way / lost))
        && fastest - slowest <= GLIDE_STEADY_MS
}
