//! What happened in a recorded flight: where it took off and landed, every
//! thermal and every glide, and how far it went.
//!
//! The analysis looks at the whole track at once, so unlike the cockpit it
//! can judge each moment by what came after it as well as before.

use venti_domain::GeoPoint;

use crate::igc::Fix;
use crate::live::wind::{Wind, WindEstimator};
use crate::live::{Config, FlightComputer, PositionSample};
use crate::scoring::{self, RouteKind};
use crate::track;

/// Metres per degree of latitude on the FAI sphere.
const M_PER_DEG_LAT: f64 = 111_194.9;

/// The glider is circling at a fix if its direction of travel turns at
/// least this far, in degrees, within the half minute around it: half a
/// circle. Circles in a thermal take 15 to 30 seconds each.
const CIRCLING_TURN_DEG: f64 = 180.0;
/// The half minute, in seconds either side of the fix.
const TURN_HALF_WINDOW_S: i64 = 15;
/// Stretches of circling closer together than this, in seconds, are one
/// thermal: the pilot re-centred.
const MERGE_GAP_S: i64 = 20;
/// Circling for less than this, in seconds, is a turn, not a thermal.
const MIN_THERMAL_S: i64 = 40;
/// Circling that loses height faster than this on average, in metres per
/// second, is a spiral or a descent, not a thermal.
const MIN_THERMAL_CLIMB_MS: f64 = -0.5;
/// A glide shorter than this, in seconds, is not listed.
const MIN_GLIDE_S: i64 = 30;

/// A moment of the flight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moment {
    /// Position of this moment in the list of fixes.
    pub index: usize,
    /// Seconds since the first fix.
    pub seconds: i64,
    /// Where.
    pub position: GeoPoint,
    /// Altitude in metres.
    pub altitude_m: i32,
}

/// One thermal: a stretch of circling that did not lose height.
#[derive(Debug, Clone, PartialEq)]
pub struct Thermal {
    /// Where circling began.
    pub entry: Moment,
    /// Where it ended.
    pub exit: Moment,
    /// Height gained from entry to exit, metres.
    pub gain_m: i32,
    /// Average climb, metres per second.
    pub climb_ms: f64,
    /// The wind, worked out from the drift of the circles.
    pub wind: Option<Wind>,
}

/// One glide: the flying between two thermals, or before the first, or
/// after the last.
#[derive(Debug, Clone, PartialEq)]
pub struct Glide {
    /// Where it began.
    pub start: Moment,
    /// Where it ended.
    pub end: Moment,
    /// Distance from start to end in a straight line, metres.
    pub distance_m: f64,
    /// Height lost, metres. Negative if the glide gained height.
    pub height_lost_m: i32,
    /// Metres forward per metre lost. `None` if the glide did not lose
    /// height.
    pub glide_ratio: Option<f64>,
    /// Average speed over the ground along the straight line, metres per
    /// second.
    pub speed_ms: f64,
}

/// How the flight scores under cross-country league rules; see
/// [`crate::scoring`].
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredRoute {
    /// What the flight is scored as.
    pub kind: RouteKind,
    /// For a triangle, whether it is closed to within 5%.
    pub closed: bool,
    /// The scoring distance, metres.
    pub distance_m: f64,
    /// The points.
    pub points: f64,
    /// The points the route runs through, in flight order: start,
    /// turnpoints, finish.
    pub points_on_track: Vec<Moment>,
    /// For a triangle, the gap between start and finish, metres.
    pub closing_m: Option<f64>,
}

/// What happened in a flight.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// Where the glider took off. `None` if the recording never shows it
    /// flying.
    pub takeoff: Option<Moment>,
    /// Where it landed. `None` if the recording ends in the air.
    pub landing: Option<Moment>,
    /// Seconds from take-off to landing, or to the end of the recording.
    pub airtime_s: i64,
    /// Every thermal, in order.
    pub thermals: Vec<Thermal>,
    /// Every glide, in order.
    pub glides: Vec<Glide>,
    /// Seconds spent in thermals.
    pub thermalling_s: i64,
    /// Seconds spent on glides.
    pub gliding_s: i64,
    /// Height gained in thermals, metres.
    pub height_gained_m: i32,
    /// How the flight scores under cross-country league rules.
    pub score: Option<ScoredRoute>,
}

/// The part of a recording in which the glider flew: from take-off to
/// landing, or to the end if it never landed.
///
/// A recorder that is switched on at the car park and off at the bar writes
/// a lot that is not flight. Everything shown about a flight is worked out
/// on this part only. A recording in which the glider never flies is
/// returned whole, so that it still shows as what it is.
///
/// A recording that holds several flights counts as its longest; see
/// `takeoff_and_landing`. Whether to split such recordings is open.
pub fn flown_part(fixes: &[Fix]) -> &[Fix] {
    match takeoff_and_landing(fixes) {
        (Some(takeoff), landing) => {
            &fixes[takeoff..=landing.unwrap_or(fixes.len() - 1).max(takeoff)]
        }
        _ => fixes,
    }
}

/// Analyses a recording.
///
/// Take-off and landing are looked for in the whole recording. Everything
/// else is worked out on its [`flown_part`], and every [`Moment`] counts
/// from there: index 0 and second 0 are the take-off.
pub fn analyse(recording: &[Fix]) -> Analysis {
    let (takeoff, landing) = takeoff_and_landing(recording);
    let fixes = flown_part(recording);
    let altitudes = track::altitudes(fixes);
    let moment = |index: usize| Moment {
        index,
        seconds: (fixes[index].time - fixes[0].time).num_seconds(),
        position: fixes[index].position,
        altitude_m: altitudes[index],
    };

    let (first, last) = (0, fixes.len().saturating_sub(1));
    if fixes.is_empty() || takeoff.is_none() {
        return Analysis {
            takeoff: None,
            landing: None,
            airtime_s: 0,
            thermals: Vec::new(),
            glides: Vec::new(),
            thermalling_s: 0,
            gliding_s: 0,
            height_gained_m: 0,
            score: None,
        };
    }

    let velocities = velocities(fixes);
    let thermals: Vec<Thermal> = circling_stretches(fixes, &velocities, first, last)
        .into_iter()
        .filter_map(|(start, end)| {
            let (entry, exit) = (moment(start), moment(end));
            let seconds = exit.seconds - entry.seconds;
            let gain_m = exit.altitude_m - entry.altitude_m;
            let climb_ms = f64::from(gain_m) / seconds as f64;
            (seconds >= MIN_THERMAL_S && climb_ms >= MIN_THERMAL_CLIMB_MS).then(|| {
                let mut estimator = WindEstimator::new();
                for index in start..=end {
                    let (east, north) = velocities[index];
                    estimator.add(
                        (fixes[index].time - fixes[0].time).num_seconds() as f64,
                        east,
                        north,
                    );
                }
                Thermal {
                    entry,
                    exit,
                    gain_m,
                    climb_ms,
                    wind: estimator.wind(),
                }
            })
        })
        .collect();

    // The glides are what lies between the thermals.
    let mut glides = Vec::new();
    let mut from = first;
    for edge in thermals
        .iter()
        .map(|thermal| (thermal.entry.index, thermal.exit.index))
        .chain([(last, last)])
    {
        let (start, end) = (moment(from), moment(edge.0));
        let seconds = end.seconds - start.seconds;
        if seconds >= MIN_GLIDE_S {
            let distance_m = start.position.distance_m(end.position);
            let height_lost_m = start.altitude_m - end.altitude_m;
            glides.push(Glide {
                start,
                end,
                distance_m,
                height_lost_m,
                glide_ratio: (height_lost_m > 0).then(|| distance_m / f64::from(height_lost_m)),
                speed_ms: distance_m / seconds as f64,
            });
        }
        from = edge.1;
    }

    Analysis {
        takeoff: Some(moment(first)),
        landing: landing.map(|_| moment(last)),
        airtime_s: moment(last).seconds,
        thermalling_s: thermals
            .iter()
            .map(|thermal| thermal.exit.seconds - thermal.entry.seconds)
            .sum(),
        gliding_s: glides
            .iter()
            .map(|glide| glide.end.seconds - glide.start.seconds)
            .sum(),
        height_gained_m: thermals.iter().map(|thermal| thermal.gain_m.max(0)).sum(),
        score: scoring::score(fixes).map(|score| ScoredRoute {
            kind: score.kind,
            closed: score.closed,
            distance_m: score.distance_m,
            points: score.points,
            points_on_track: score.route.into_iter().map(moment).collect(),
            closing_m: score.closing_m,
        }),
        thermals,
        glides,
    }
}

/// How long a glider must have been at rest when its recording ends for
/// the end to be its landing, seconds. A recorder that ends a flight by
/// itself does so some ten seconds after the glider has stopped, which is
/// sooner than the cockpit's own rule would call a landing.
const AT_REST_AT_THE_END_S: f64 = 3.0;

/// A stretch of a recording whose faster tenth is faster than this over
/// the ground, metres per second (80 km/h), was driven and not flown: a
/// recorder left running on the way home. A paraglider reaches that speed
/// with a strong wind behind it and does not hold it for a tenth of a
/// flight. To verify with real recordings of drives.
const DRIVEN_SPEED_MS: f64 = 22.0;

/// The fixes at which the glider took off and landed, by the cockpit's own
/// rules: the track is played through the flight computer.
///
/// A recording can hold more than one flight: a hop before the flight, a
/// second flight after a walk up, the drive home. The flight is the
/// longest of them that was flown. Splitting a recording into its flights
/// is not built.
fn takeoff_and_landing(fixes: &[Fix]) -> (Option<usize>, Option<usize>) {
    let mut flights: Vec<(usize, Option<usize>)> = Vec::new();
    let mut from = 0;
    while from < fixes.len() {
        let (Some(takeoff), landing, looked_at) = first_flight(&fixes[from..]) else {
            break;
        };
        flights.push((from + takeoff, landing.map(|landing| from + landing)));
        if landing.is_none() {
            break;
        }
        from += looked_at.max(1);
    }
    let seconds = |&(takeoff, landing): &(usize, Option<usize>)| {
        let last = landing.unwrap_or(fixes.len() - 1).max(takeoff);
        (fixes[last].time - fixes[takeoff].time).num_seconds()
    };
    let flown = |&&(takeoff, landing): &&(usize, Option<usize>)| {
        !was_driven(&fixes[takeoff..=landing.unwrap_or(fixes.len() - 1).max(takeoff)])
    };
    flights
        .iter()
        .filter(flown)
        .max_by_key(|flight| seconds(flight))
        // Nothing but what looks driven: the first of it, as it was read
        // before recordings were looked through to their end.
        .or(flights.first())
        .map_or((None, None), |&(takeoff, landing)| (Some(takeoff), landing))
}

/// The first flight in a stretch of a recording: where the glider took off
/// and landed, and how many fixes were looked at to say so.
///
/// A landing the computer takes back, because the glider flew on, is not
/// one: the flight ends at the landing that stood. A recording that ends
/// with the glider at rest, before the computer has called it landed, ends
/// with a landing too.
fn first_flight(fixes: &[Fix]) -> (Option<usize>, Option<usize>, usize) {
    let config = Config::default();
    let confirm_s = config.landing_confirm_s;
    let mut computer = FlightComputer::new(config);
    let (mut takeoff_s, mut landing_s) = (None, None);
    let mut looked_at = fixes.len();
    for (index, fix) in fixes.iter().enumerate() {
        let time = fix.time.timestamp() as f64;
        computer.position(PositionSample {
            time_s: time,
            position: fix.position,
            altitude_m: fix.valid.then(|| f64::from(fix.gps_altitude_m)),
            ground_speed_ms: None,
            course_deg: None,
        });
        let state = computer.state(time);
        takeoff_s = state.takeoff_time_s;
        landing_s = state.landing_time_s;
        // A landing stands once the time in which it can be taken back is
        // over: what follows is another flight, or none.
        if landing_s.is_some_and(|landed| time > landed + confirm_s) {
            looked_at = index;
            break;
        }
    }
    if landing_s.is_none()
        && let (Some(since), Some(last)) = (computer.at_rest_since(), fixes.last())
        && last.time.timestamp() as f64 - since >= AT_REST_AT_THE_END_S
    {
        landing_s = Some(since);
    }
    let index_at = |time_s: f64| {
        fixes
            .iter()
            .position(|fix| fix.time.timestamp() as f64 >= time_s)
    };
    (
        takeoff_s.and_then(index_at),
        landing_s.and_then(index_at),
        looked_at,
    )
}

/// Whether a stretch of a recording was driven: see [`DRIVEN_SPEED_MS`].
fn was_driven(fixes: &[Fix]) -> bool {
    // Speeds over about ten seconds, so that one wild fix says nothing.
    let mut speeds: Vec<f64> = fixes
        .chunks(10)
        .filter_map(|chunk| {
            let (first, last) = (chunk.first()?, chunk.last()?);
            let seconds = (last.time - first.time).num_seconds();
            (seconds > 0).then(|| first.position.distance_m(last.position) / seconds as f64)
        })
        .collect();
    if speeds.len() < 10 {
        return false;
    }
    speeds.sort_by(f64::total_cmp);
    speeds[speeds.len() * 9 / 10] > DRIVEN_SPEED_MS
}

/// Ground velocity at each fix as east and north parts in metres per
/// second, from the step to the next fix.
fn velocities(fixes: &[Fix]) -> Vec<(f64, f64)> {
    let mut velocities: Vec<(f64, f64)> = fixes
        .windows(2)
        .map(|pair| {
            let seconds = (pair[1].time - pair[0].time).num_seconds().max(1) as f64;
            let (from, to) = (pair[0].position, pair[1].position);
            let north = (to.lat() - from.lat()) * M_PER_DEG_LAT;
            let east = (to.lon() - from.lon()) * M_PER_DEG_LAT * from.lat().to_radians().cos();
            (east / seconds, north / seconds)
        })
        .collect();
    if let Some(&last) = velocities.last() {
        velocities.push(last);
    }
    velocities
}

/// The stretches between `first` and `last` in which the glider circles,
/// as pairs of fix positions.
fn circling_stretches(
    fixes: &[Fix],
    velocities: &[(f64, f64)],
    first: usize,
    last: usize,
) -> Vec<(usize, usize)> {
    let seconds: Vec<i64> = fixes
        .iter()
        .map(|fix| (fix.time - fixes[0].time).num_seconds())
        .collect();
    // The turn from each fix to the next, in degrees, left negative. A
    // glider barely moving has no direction worth the name.
    let turns: Vec<f64> = velocities
        .windows(2)
        .map(|pair| {
            let moving = |v: (f64, f64)| v.0.hypot(v.1) > 1.0;
            if !moving(pair[0]) || !moving(pair[1]) {
                return 0.0;
            }
            let course = |v: (f64, f64)| v.0.atan2(v.1).to_degrees();
            (course(pair[1]) - course(pair[0]) + 540.0).rem_euclid(360.0) - 180.0
        })
        .chain([0.0])
        .collect();

    // Running total of turn, so the turn within any window is a subtraction.
    let mut total = vec![0.0];
    for turn in &turns {
        total.push(total[total.len() - 1] + turn);
    }

    let mut stretches: Vec<(usize, usize)> = Vec::new();
    let (mut from, mut to) = (first, first);
    for index in first..=last {
        while seconds[from] < seconds[index] - TURN_HALF_WINDOW_S {
            from += 1;
        }
        while to < last && seconds[to + 1] <= seconds[index] + TURN_HALF_WINDOW_S {
            to += 1;
        }
        if (total[to] - total[from]).abs() < CIRCLING_TURN_DEG {
            continue;
        }
        match stretches.last_mut() {
            Some(stretch) if seconds[index] - seconds[stretch.1] <= MERGE_GAP_S => {
                stretch.1 = index
            }
            _ => stretches.push((index, index)),
        }
    }
    stretches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::synthetic_flight;

    // The made-up flight: a glide out to 90 s, a thermal climbing 2.4 m/s
    // to 342 s, a glide to 722 s, a thermal climbing 1.6 m/s to 890 s, two
    // glides to 1520 s, then a spiral down at 1.9 m/s to the end at 1744 s.

    #[test]
    fn finds_both_thermals_and_measures_them() {
        let analysis = analyse(&synthetic_flight().fixes);

        assert_eq!(analysis.thermals.len(), 2, "{:#?}", analysis.thermals);
        let first = &analysis.thermals[0];
        assert!((first.entry.seconds - 90).abs() <= 12, "{first:?}");
        assert!((first.exit.seconds - 342).abs() <= 12, "{first:?}");
        assert!((first.climb_ms - 2.4).abs() < 0.15, "{first:?}");
        assert!((first.gain_m - 605).abs() < 40, "{first:?}");
        // The made-up thermals drift east at 3 m/s: a westerly.
        let wind = first.wind.unwrap();
        assert!((wind.from_deg - 270.0).abs() < 8.0 && (wind.speed_ms - 3.0).abs() < 0.5);

        let second = &analysis.thermals[1];
        assert!((second.entry.seconds - 722).abs() <= 12, "{second:?}");
        assert!((second.climb_ms - 1.6).abs() < 0.15, "{second:?}");
        assert!((analysis.height_gained_m - (605 + 269)).abs() < 60);
        assert!((analysis.thermalling_s - (252 + 168)).abs() <= 30);
    }

    #[test]
    fn the_spiral_down_is_not_a_thermal_and_ends_the_last_glide() {
        let analysis = analyse(&synthetic_flight().fixes);
        // Glides: out from the take-off, between the thermals, and after the
        // second thermal until the recording ends (the spiral included,
        // since nothing separates it).
        assert_eq!(analysis.glides.len(), 3, "{:#?}", analysis.glides);
        let between = &analysis.glides[1];
        // 380 s at 11 m/s, sinking 1.1 m/s: ten to one.
        assert!(
            (between.glide_ratio.unwrap() - 10.0).abs() < 0.6,
            "{between:?}"
        );
        assert!((between.speed_ms - 11.0).abs() < 0.6, "{between:?}");
        assert!((between.height_lost_m - 418).abs() < 40, "{between:?}");
    }

    #[test]
    fn dates_the_take_off_and_sees_that_the_recording_ends_in_the_air() {
        let flight = synthetic_flight();
        let analysis = analyse(&flight.fixes);

        let takeoff = analysis.takeoff.unwrap();
        assert_eq!((takeoff.index, takeoff.seconds), (0, 0));
        assert_eq!(takeoff.position, flight.fixes[0].position);
        assert_eq!(analysis.landing, None);
        assert_eq!(analysis.airtime_s, 1744);
    }

    #[test]
    fn a_recording_with_ground_time_is_trimmed_to_the_flight() {
        // Two minutes standing, the made-up flight, then two minutes
        // standing where it ended.
        let flight = synthetic_flight();
        let (first, last) = (flight.fixes[0], *flight.fixes.last().unwrap());
        let mut fixes: Vec<Fix> = (0..120)
            .map(|second| Fix {
                time: first.time - chrono::Duration::seconds(120 - second),
                ..first
            })
            .collect();
        fixes.extend(&flight.fixes);
        fixes.extend((1..=120).map(|second| Fix {
            time: last.time + chrono::Duration::seconds(second),
            ..last
        }));

        let analysis = analyse(&fixes);

        // Moments count from the take-off, wherever in the recording it is.
        let takeoff = analysis.takeoff.unwrap();
        assert_eq!((takeoff.index, takeoff.seconds), (0, 0));
        assert_eq!(takeoff.position, flight.fixes[0].position);
        let landing = analysis.landing.unwrap();
        assert!((landing.seconds - 1744).abs() <= 12, "{landing:?}");
        assert_eq!(analysis.airtime_s, landing.seconds);
        assert_eq!(analysis.thermals.len(), 2);
        assert!((analysis.thermals[0].entry.seconds - 90).abs() <= 12);
    }

    /// Fixes of standing still where `at` is, one a second, after it.
    fn standing(at: Fix, seconds: i64) -> Vec<Fix> {
        (1..=seconds)
            .map(|second| Fix {
                time: at.time + chrono::Duration::seconds(second),
                ..at
            })
            .collect()
    }

    /// The same fixes, beginning one second after `after`.
    fn later(fixes: &[Fix], after: Fix) -> Vec<Fix> {
        let shift = after.time + chrono::Duration::seconds(1) - fixes[0].time;
        fixes
            .iter()
            .map(|fix| Fix {
                time: fix.time + shift,
                ..*fix
            })
            .collect()
    }

    #[test]
    fn a_recording_that_stops_as_the_glider_stops_ends_with_a_landing() {
        // A recorder that ends the flight by itself writes some ten
        // seconds of standing and no more: sooner than the cockpit would
        // call a landing, and a landing all the same.
        let flight = synthetic_flight();
        let mut fixes = flight.fixes.clone();
        fixes.extend(standing(*flight.fixes.last().unwrap(), 12));

        let analysis = analyse(&fixes);

        let landing = analysis
            .landing
            .expect("the glider stood when the recording ended");
        assert!((landing.seconds - 1744).abs() <= 12, "{landing:?}");
        assert_eq!(analysis.airtime_s, landing.seconds);
    }

    #[test]
    fn a_glider_that_stands_in_the_wind_and_flies_on_has_not_landed() {
        // Parked in the wind on a ridge, or down on a dune and off again:
        // forty seconds at one place and one height, then on. One flight.
        let flight = synthetic_flight();
        let mut fixes = flight.fixes.clone();
        fixes.extend(standing(*fixes.last().unwrap(), 40));
        fixes.extend(later(&flight.fixes, *fixes.last().unwrap()));
        fixes.extend(standing(*fixes.last().unwrap(), 12));

        let analysis = analyse(&fixes);

        assert_eq!(analysis.takeoff.unwrap().index, 0);
        let whole = 1744 + 40 + 1 + 1744;
        assert!(
            (analysis.airtime_s - whole).abs() <= 15,
            "{}",
            analysis.airtime_s
        );
        assert_eq!(analysis.thermals.len(), 4);
    }

    #[test]
    fn the_flight_of_a_recording_is_its_longest_and_not_the_hop_before_it() {
        // A minute in the air, five on the ground, then the flight: what
        // a recorder writes that was started for a first try.
        let flight = synthetic_flight();
        let mut fixes: Vec<Fix> = flight.fixes[..60].to_vec();
        fixes.extend(standing(*fixes.last().unwrap(), 300));
        let begins = fixes.len();
        fixes.extend(later(&flight.fixes, *fixes.last().unwrap()));
        fixes.extend(standing(*fixes.last().unwrap(), 12));

        let flown = flown_part(&fixes);

        assert_eq!(flown[0].time, fixes[begins].time);
        let seconds = (flown.last().unwrap().time - flown[0].time).num_seconds();
        assert!((seconds - 1744).abs() <= 15, "{seconds}");
        assert_eq!(analyse(&fixes).thermals.len(), 2);
    }

    #[test]
    fn the_drive_home_in_a_recorder_left_running_is_not_the_flight() {
        // The flight, five minutes of packing, then forty minutes on the
        // road at 90 km/h: longer than the flight, and not flown.
        let flight = synthetic_flight();
        let mut fixes = flight.fixes.clone();
        fixes.extend(standing(*fixes.last().unwrap(), 300));
        let parked = *fixes.last().unwrap();
        fixes.extend((1..=2400_i32).map(|second| {
            Fix {
                time: parked.time + chrono::Duration::seconds(i64::from(second)),
                position: GeoPoint::new(
                    parked.position.lat() + f64::from(second) * 25.0 / M_PER_DEG_LAT,
                    parked.position.lon(),
                )
                .unwrap(),
                ..parked
            }
        }));

        let analysis = analyse(&fixes);

        assert_eq!(analysis.takeoff.unwrap().index, 0);
        let landing = analysis.landing.unwrap();
        assert!((landing.seconds - 1744).abs() <= 12, "{landing:?}");
    }

    #[test]
    fn the_flown_part_leaves_out_the_standing_around() {
        let flight = synthetic_flight();
        let (first, last) = (flight.fixes[0], *flight.fixes.last().unwrap());
        let mut fixes: Vec<Fix> = (0..120)
            .map(|second| Fix {
                time: first.time - chrono::Duration::seconds(120 - second),
                ..first
            })
            .collect();
        fixes.extend(&flight.fixes);
        fixes.extend((1..=120).map(|second| Fix {
            time: last.time + chrono::Duration::seconds(second),
            ..last
        }));

        let flown = flown_part(&fixes);
        let seconds = (flown.last().unwrap().time - flown[0].time).num_seconds();
        assert!((seconds - 1744).abs() <= 15, "{seconds}");
        assert_eq!(flown[0].time, first.time);

        // A recording that never flies, or is empty, comes back whole.
        assert_eq!(flown_part(&fixes[..100]).len(), 100);
        assert!(flown_part(&[]).is_empty());
    }

    #[test]
    fn the_flight_is_scored_and_its_route_is_made_of_moments_on_the_track() {
        let flight = synthetic_flight();
        let analysis = analyse(&flight.fixes);
        let score = analysis.score.unwrap();

        let straight = flight.fixes[0]
            .position
            .distance_m(flight.fixes.last().unwrap().position);
        assert!(
            score.distance_m > straight,
            "{} <= {straight}",
            score.distance_m
        );
        assert!(score.points >= score.distance_m / 1000.0);
        assert!((2..=5).contains(&score.points_on_track.len()));
        assert!(
            score
                .points_on_track
                .windows(2)
                .all(|pair| pair[0].index <= pair[1].index)
        );
        let last = score.points_on_track.last().unwrap();
        assert_eq!(last.position, flight.fixes[last.index].position);
    }

    #[test]
    fn a_track_that_never_flies_has_no_flight_in_it() {
        let flight = synthetic_flight();
        let standing: Vec<Fix> = (0..300)
            .map(|second| Fix {
                time: flight.fixes[0].time + chrono::Duration::seconds(second),
                ..flight.fixes[0]
            })
            .collect();

        let analysis = analyse(&standing);
        assert_eq!(analysis.takeoff, None);
        assert!(analysis.thermals.is_empty() && analysis.glides.is_empty());
        assert_eq!(analysis.score, None);
        assert_eq!(analyse(&[]).airtime_s, 0);
    }
}
