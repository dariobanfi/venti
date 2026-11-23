//! How a pilot flew a flight, in figures: which way they circled, how
//! their thermals ended, how they climbed and glided.
//!
//! The figures are worked out from the thermals and glides that
//! [`crate::analysis`] finds, by fixed rules with the limits below. They
//! are counts and sums, never averages, so that the figures of many
//! flights add up to those of all of them without averaging averages.
//!
//! They are what a GPS track at about one fix a second shows and no more.
//! The direction of a turn is about as certain as anything in a track; a
//! climb over one minute is noisy by a few tenths of a metre per second,
//! since satellite heights jump by a few metres from fix to fix. Nothing
//! here knows the air the pilot was in, what they meant to do or why: the
//! figures say what was flown, not whether it was good. Every limit is to
//! verify with pilots.

use crate::analysis::{Analysis, Moment, Thermal};
use crate::geodesy::Earth;
use crate::igc::Fix;
use crate::track;

/// Fixes closer together than this, in metres, are one place for the turn
/// of the track: a GPS track jitters by a few metres from fix to fix, and
/// over short steps that jitter counts as turning both ways.
const MIN_STEP_M: f64 = 15.0;
/// A thermal is circled one way when the track's net turn in it is at
/// least this, in degrees: one full circle.
const ONE_WAY_TURN_DEG: f64 = 360.0;
/// ... and at least this share of all its turning went that way. Less, and
/// it was flown back and forth, in eights or beats along a slope, or with
/// a change of direction; with less than a full circle there is too
/// little to say. Both limits were set against some 660 thermals of real
/// flights, of which two thirds are then one way.
const ONE_WAY_SHARE: f64 = 2.0 / 3.0;
/// How a thermal ended is measured only on thermals at least this long, in
/// seconds: in a shorter one the last minute is most of it.
const MEASURED_THERMAL_S: i64 = 120;
/// ... and only on those that climbed at least this fast on average, in
/// metres per second: in a weaker one a third of the average is noise.
const MEASURED_THERMAL_CLIMB_MS: f64 = 0.5;
/// The end of a thermal is its last this many seconds.
const LAST_PART_S: i64 = 60;
/// A thermal ended weak when its last minute climbed at less than this
/// share of its own average climb.
const WEAK_END_SHARE: f64 = 1.0 / 3.0;
/// The best thermal of a flight is chosen among those at least this long,
/// in seconds: a shorter one is a gust as often as a climb.
const BEST_THERMAL_S: i64 = 60;
/// Glides are counted from this length, in metres, straight from start to
/// end: a shorter one is mostly the way into the next thermal.
const COUNTED_GLIDE_M: f64 = 1000.0;

/// How a flight was flown, in counts and sums. Add the figures of several
/// flights with `+=` for the figures of all of them.
///
/// The server keeps them with each flight as JSON of these field names
/// (`venti-store`): a field renamed here is a flight's figures lost until
/// its analysis is run again.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct FlightHabits {
    /// Thermals circled to the left, by the rule of [`ONE_WAY_TURN_DEG`]
    /// and [`ONE_WAY_SHARE`].
    pub thermals_left: u32,
    /// Thermals circled to the right.
    pub thermals_right: u32,
    /// Thermals circled both ways, or turned too little to say.
    pub thermals_both_ways: u32,
    /// Seconds in the thermals circled to the left.
    pub circling_left_s: i64,
    /// Seconds in the thermals circled to the right.
    pub circling_right_s: i64,
    /// Thermals whose end was measured: long and strong enough, see
    /// [`MEASURED_THERMAL_S`].
    pub thermals_measured: u32,
    /// Of those, the ones whose last minute climbed at less than a third
    /// of their average.
    pub thermals_ended_weak: u32,
    /// Over the measured thermals, the seconds between reaching each one's
    /// highest point and leaving it, together.
    pub after_the_top_s: i64,
    /// Over the measured thermals, the height between each one's highest
    /// point and its exit, metres, together.
    pub below_the_top_m: i64,
    /// Height gained in thermals, metres. A thermal that lost height adds
    /// nothing.
    pub height_gained_m: i64,
    /// Seconds in thermals.
    pub thermalling_s: i64,
    /// The best average climb of a thermal of at least a minute, metres per
    /// second. `None` if there was no such thermal.
    pub best_climb_ms: Option<f64>,
    /// Glides of at least a kilometre that lost height.
    pub glides: u32,
    /// Their distances together, metres, each in a straight line.
    pub glide_distance_m: f64,
    /// The height they lost together, metres.
    pub glide_height_lost_m: i64,
    /// Seconds on glides, all of them.
    pub gliding_s: i64,
    /// Seconds in the air.
    pub airtime_s: i64,
}

impl FlightHabits {
    /// Every thermal counted for which way it was circled.
    pub fn thermals(&self) -> u32 {
        self.thermals_left + self.thermals_right + self.thermals_both_ways
    }
}

impl std::ops::AddAssign for FlightHabits {
    fn add_assign(&mut self, other: Self) {
        self.thermals_left += other.thermals_left;
        self.thermals_right += other.thermals_right;
        self.thermals_both_ways += other.thermals_both_ways;
        self.circling_left_s += other.circling_left_s;
        self.circling_right_s += other.circling_right_s;
        self.thermals_measured += other.thermals_measured;
        self.thermals_ended_weak += other.thermals_ended_weak;
        self.after_the_top_s += other.after_the_top_s;
        self.below_the_top_m += other.below_the_top_m;
        self.height_gained_m += other.height_gained_m;
        self.thermalling_s += other.thermalling_s;
        self.best_climb_ms = match (self.best_climb_ms, other.best_climb_ms) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        self.glides += other.glides;
        self.glide_distance_m += other.glide_distance_m;
        self.glide_height_lost_m += other.glide_height_lost_m;
        self.gliding_s += other.gliding_s;
        self.airtime_s += other.airtime_s;
    }
}

/// Which way a thermal was circled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Left,
    Right,
    BothWays,
}

/// The figures of one flight.
///
/// `fixes` are the fixes the analysis counts its moments from: the
/// [`flown_part`](crate::analysis::flown_part) of the recording that was
/// analysed. A thermal whose moments are not among them is left out.
pub fn habits(fixes: &[Fix], analysis: &Analysis) -> FlightHabits {
    let altitudes = track::altitudes(fixes);
    let seconds: Vec<i64> = fixes
        .iter()
        .map(|fix| (fix.time - fixes[0].time).num_seconds())
        .collect();
    let on_track = |moment: &Moment| {
        fixes
            .get(moment.index)
            .is_some_and(|fix| fix.position == moment.position)
    };

    let mut habits = FlightHabits {
        thermalling_s: analysis.thermalling_s,
        gliding_s: analysis.gliding_s,
        airtime_s: analysis.airtime_s,
        ..FlightHabits::default()
    };

    for thermal in analysis
        .thermals
        .iter()
        .filter(|thermal| on_track(&thermal.entry) && on_track(&thermal.exit))
    {
        let duration_s = thermal.exit.seconds - thermal.entry.seconds;
        match direction(&fixes[thermal.entry.index..=thermal.exit.index]) {
            Direction::Left => {
                habits.thermals_left += 1;
                habits.circling_left_s += duration_s;
            }
            Direction::Right => {
                habits.thermals_right += 1;
                habits.circling_right_s += duration_s;
            }
            Direction::BothWays => habits.thermals_both_ways += 1,
        }

        if let Some(end) = ending(thermal, &altitudes, &seconds) {
            habits.thermals_measured += 1;
            habits.thermals_ended_weak += u32::from(end.weak);
            habits.after_the_top_s += end.after_the_top_s;
            habits.below_the_top_m += end.below_the_top_m;
        }

        habits.height_gained_m += i64::from(thermal.gain_m.max(0));
        if duration_s >= BEST_THERMAL_S {
            habits.best_climb_ms = Some(
                habits
                    .best_climb_ms
                    .map_or(thermal.climb_ms, |best| best.max(thermal.climb_ms)),
            );
        }
    }

    for glide in &analysis.glides {
        if glide.distance_m >= COUNTED_GLIDE_M && glide.height_lost_m > 0 {
            habits.glides += 1;
            habits.glide_distance_m += glide.distance_m;
            habits.glide_height_lost_m += i64::from(glide.height_lost_m);
        }
    }
    habits
}

/// Which way the track turns over these fixes: the change of its direction
/// from each step to the next, steps shorter than [`MIN_STEP_M`] left out.
fn direction(fixes: &[Fix]) -> Direction {
    let (mut left, mut right) = (0.0_f64, 0.0_f64);
    let mut from = match fixes.first() {
        Some(fix) => fix.position,
        None => return Direction::BothWays,
    };
    let mut course: Option<f64> = None;
    for fix in &fixes[1..] {
        let step = Earth::FaiSphere.course(from, fix.position);
        if step.distance_m < MIN_STEP_M {
            continue;
        }
        if let Some(before) = course {
            // Clockwise is positive: a turn to the right.
            let turn = (step.bearing_deg - before + 540.0).rem_euclid(360.0) - 180.0;
            if turn > 0.0 {
                right += turn;
            } else {
                left -= turn;
            }
        }
        course = Some(step.bearing_deg);
        from = fix.position;
    }
    let (net, all) = (right - left, right + left);
    if net.abs() < ONE_WAY_TURN_DEG || net.abs() < ONE_WAY_SHARE * all {
        Direction::BothWays
    } else if net > 0.0 {
        Direction::Right
    } else {
        Direction::Left
    }
}

/// How a measured thermal ended.
struct Ending {
    /// Whether its last minute climbed at less than a third of its average.
    weak: bool,
    /// Seconds from its highest point to its exit.
    after_the_top_s: i64,
    /// Metres from its highest point down to its exit.
    below_the_top_m: i64,
}

/// How a thermal ended, if it is one to measure.
fn ending(thermal: &Thermal, altitudes: &[i32], seconds: &[i64]) -> Option<Ending> {
    let (entry, exit) = (thermal.entry.index, thermal.exit.index);
    let duration_s = thermal.exit.seconds - thermal.entry.seconds;
    if duration_s < MEASURED_THERMAL_S || thermal.climb_ms < MEASURED_THERMAL_CLIMB_MS {
        return None;
    }
    // The last fix at least a minute before the exit.
    let before = (entry..exit)
        .rev()
        .find(|&index| seconds[index] <= seconds[exit] - LAST_PART_S)?;
    let last_climb_ms =
        f64::from(altitudes[exit] - altitudes[before]) / (seconds[exit] - seconds[before]) as f64;
    // The first time the thermal's highest point was reached: time spent
    // level at the top counts as time after it.
    let top = altitudes[entry..=exit]
        .iter()
        .enumerate()
        .max_by_key(|&(offset, altitude)| (*altitude, std::cmp::Reverse(offset)))
        .map_or(exit, |(offset, _)| entry + offset);
    Some(Ending {
        weak: last_climb_ms < WEAK_END_SHARE * thermal.climb_ms,
        after_the_top_s: seconds[exit] - seconds[top],
        below_the_top_m: i64::from(altitudes[top] - altitudes[exit]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{analyse, flown_part};
    use crate::testing::{Leg, made_up_flight, mirrored, synthetic_flight};

    fn habits_of(fixes: &[Fix]) -> FlightHabits {
        habits(flown_part(fixes), &analyse(fixes))
    }

    fn glide(seconds: i64) -> Leg {
        Leg::Glide {
            seconds,
            heading: 300.0,
            speed: 11.0,
            climb: -1.1,
        }
    }

    /// The flights one after the other, each carrying on from where and
    /// when the one before ended.
    fn joined(flights: &[crate::igc::IgcFile]) -> Vec<Fix> {
        let mut fixes: Vec<Fix> = Vec::new();
        for flight in flights {
            let Some(&last) = fixes.last() else {
                fixes.extend(&flight.fixes);
                continue;
            };
            let first = flight.fixes[0];
            fixes.extend(flight.fixes[1..].iter().map(|fix| {
                let height = last.gps_altitude_m - first.gps_altitude_m;
                Fix {
                    time: last.time + (fix.time - first.time),
                    position: venti_domain::GeoPoint::new(
                        fix.position.lat() + last.position.lat() - first.position.lat(),
                        fix.position.lon() + last.position.lon() - first.position.lon(),
                    )
                    .unwrap(),
                    gps_altitude_m: fix.gps_altitude_m + height,
                    pressure_altitude_m: fix.pressure_altitude_m + height,
                    ..*fix
                }
            }));
        }
        fixes
    }

    // The made-up flight: a glide out, a thermal of 252 s at 2.4 m/s, a
    // glide of 380 s, a thermal of 168 s at 1.6 m/s, two glides that run
    // up the valley and back, and a spiral down. Its circles turn right.

    #[test]
    fn the_made_up_flight_circles_right_and_its_mirror_left() {
        let right = habits_of(&synthetic_flight().fixes);
        assert_eq!(
            (
                right.thermals_left,
                right.thermals_right,
                right.thermals_both_ways
            ),
            (0, 2, 0),
            "{right:?}"
        );
        assert_eq!(right.circling_left_s, 0);
        assert!(
            (right.circling_right_s - (252 + 168)).abs() <= 30,
            "{right:?}"
        );
        assert_eq!(right.circling_right_s, right.thermalling_s);

        let left = habits_of(&mirrored(&synthetic_flight()).fixes);
        assert_eq!(
            (
                left.thermals_left,
                left.thermals_right,
                left.thermals_both_ways
            ),
            (2, 0, 0),
            "{left:?}"
        );
        assert_eq!(left.circling_left_s, right.circling_right_s);
        assert_eq!(left.circling_right_s, 0);
    }

    #[test]
    fn a_thermal_circled_one_way_and_then_the_other_is_both_ways() {
        // Four circles to the right, then four to the left: the net turn
        // is nothing.
        let circles = || {
            made_up_flight(&[Leg::Circles {
                circles: 4,
                climb: 2.0,
            }])
        };
        let fixes = joined(&[
            made_up_flight(&[glide(60)]),
            circles(),
            mirrored(&circles()),
            made_up_flight(&[glide(120)]),
        ]);

        let habits = habits_of(&fixes);
        assert_eq!(habits.thermals(), 1, "{habits:?}");
        assert_eq!(habits.thermals_both_ways, 1, "{habits:?}");
        assert_eq!((habits.circling_left_s, habits.circling_right_s), (0, 0));
    }

    #[test]
    fn the_made_up_thermals_climb_to_their_end() {
        let habits = habits_of(&synthetic_flight().fixes);
        // Both are long and strong enough to be measured, and neither
        // weakens: the exit, found a few seconds into the glide after,
        // is the only time after the top.
        assert_eq!(habits.thermals_measured, 2, "{habits:?}");
        assert_eq!(habits.thermals_ended_weak, 0, "{habits:?}");
        assert!(habits.after_the_top_s <= 2 * 15, "{habits:?}");
        // A few seconds of the glide after, at 1.1 m/s, below each top.
        assert!((0..=2 * 20).contains(&habits.below_the_top_m), "{habits:?}");
        assert!((habits.height_gained_m - (605 + 269)).abs() < 60);
        assert!((habits.best_climb_ms.unwrap() - 2.4).abs() < 0.15);
    }

    #[test]
    fn a_thermal_that_dies_in_its_last_minute_ended_weak() {
        // Eight circles at 2 m/s, then three at nothing before leaving.
        let flight = made_up_flight(&[
            glide(60),
            Leg::Circles {
                circles: 8,
                climb: 2.0,
            },
            Leg::Circles {
                circles: 3,
                climb: 0.0,
            },
            glide(200),
        ]);
        let habits = habits_of(&flight.fixes);

        assert_eq!(habits.thermals(), 1, "{habits:?}");
        assert_eq!(
            (habits.thermals_measured, habits.thermals_ended_weak),
            (1, 1),
            "{habits:?}"
        );
        // The top was reached after the eighth circle, 84 s of circling
        // before the exit, which the analysis finds within a few seconds of
        // the end of the circles.
        assert!(
            (84 - 15..=84 + 15).contains(&habits.after_the_top_s),
            "{habits:?}"
        );
        // It held its height at the top: little below it, from the glide.
        assert!((0..=20).contains(&habits.below_the_top_m), "{habits:?}");
    }

    #[test]
    fn a_thermal_circled_on_as_it_sank_is_left_below_its_top() {
        // Eight circles at 2 m/s, then two sinking at 1 m/s: 56 m below
        // the top when the circling ends.
        let flight = made_up_flight(&[
            glide(60),
            Leg::Circles {
                circles: 8,
                climb: 2.0,
            },
            Leg::Circles {
                circles: 2,
                climb: -1.0,
            },
            glide(200),
        ]);
        let habits = habits_of(&flight.fixes);

        assert_eq!(habits.thermals_measured, 1, "{habits:?}");
        assert!(
            (56 - 15..=56 + 20).contains(&habits.below_the_top_m),
            "{habits:?}"
        );
    }

    #[test]
    fn a_short_thermal_is_counted_for_its_direction_and_not_measured() {
        // Three circles: 84 s, under the two minutes a measured one needs.
        let flight = made_up_flight(&[
            glide(60),
            Leg::Circles {
                circles: 3,
                climb: 2.0,
            },
            glide(200),
        ]);
        let habits = habits_of(&flight.fixes);

        assert_eq!(habits.thermals_right, 1, "{habits:?}");
        assert_eq!(habits.thermals_measured, 0, "{habits:?}");
        assert_eq!(habits.after_the_top_s, 0);
        assert_eq!(habits.below_the_top_m, 0);
        // At over a minute it is still the best thermal of the flight.
        assert!((habits.best_climb_ms.unwrap() - 2.0).abs() < 0.3);
    }

    #[test]
    fn only_glides_of_a_kilometre_that_lost_height_are_counted() {
        let flight = synthetic_flight();
        let analysis = analyse(&flight.fixes);
        let habits = habits(flown_part(&flight.fixes), &analysis);

        // The glide out is 945 m, the glide between the thermals 4180 m,
        // and the last runs up the valley and back to near where it began.
        let counted: Vec<_> = analysis
            .glides
            .iter()
            .filter(|glide| glide.distance_m >= 1000.0 && glide.height_lost_m > 0)
            .collect();
        assert_eq!(habits.glides as usize, counted.len());
        assert_eq!(habits.glides, 1, "{:#?}", analysis.glides);
        assert!(
            (habits.glide_distance_m - 4180.0).abs() < 200.0,
            "{habits:?}"
        );
        assert!((habits.glide_height_lost_m - 418).abs() < 40, "{habits:?}");
        assert_eq!(habits.gliding_s, analysis.gliding_s);
        assert_eq!(habits.airtime_s, 1744);
    }

    #[test]
    fn flights_add_up() {
        let one = habits_of(&synthetic_flight().fixes);
        let mut two = one;
        two += habits_of(&mirrored(&synthetic_flight()).fixes);

        assert_eq!(two.thermals(), 4);
        assert_eq!((two.thermals_left, two.thermals_right), (2, 2));
        assert_eq!(two.height_gained_m, 2 * one.height_gained_m);
        assert_eq!(two.best_climb_ms, one.best_climb_ms);
        assert_eq!(two.airtime_s, 2 * 1744);
    }

    #[test]
    fn an_empty_flight_has_no_figures() {
        assert_eq!(habits_of(&[]), FlightHabits::default());
        let mut sum = FlightHabits::default();
        sum += FlightHabits::default();
        assert_eq!(sum, FlightHabits::default());
        assert_eq!(sum.best_climb_ms, None);
    }
}
