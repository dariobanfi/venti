//! The flight computer itself: sensors in, the state of the flight out.

use std::collections::VecDeque;

use venti_domain::GeoPoint;

use super::atmosphere::pressure_altitude_m;
use super::vario::VarioFilter;
use super::wind::{Wind, WindEstimator};

/// Metres per degree of latitude on the FAI sphere.
const M_PER_DEG_LAT: f64 = 111_194.9;

/// The settings of the flight computer.
///
/// The circling rule is the one XCTrack documents; the take-off and landing
/// rules are our own. To verify in the air, all of them.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Seconds over which the average climb is taken.
    pub averaging_s: f64,
    /// The glider has taken off once it has moved at least this fast over
    /// the ground, metres per second, for `takeoff_hold_s` seconds, or has
    /// left its starting height by `takeoff_height_m`. A walking pilot stays
    /// under 2 m/s; a paraglider flies at 8 or more, less into a strong
    /// headwind, which is what the height rule is for.
    pub takeoff_speed_ms: f64,
    /// See `takeoff_speed_ms`.
    pub takeoff_hold_s: f64,
    /// See `takeoff_speed_ms`.
    pub takeoff_height_m: f64,
    /// The glider has landed once it has moved slower than this over the
    /// ground, metres per second, without climbing or sinking, for
    /// `landing_hold_s` seconds.
    pub landing_speed_ms: f64,
    /// See `landing_speed_ms`.
    pub landing_hold_s: f64,
    /// Circling starts when the direction of travel has turned at least this
    /// many degrees within `turn_window_s` seconds while the average climb
    /// is no worse than `circling_min_climb_ms`.
    pub circling_start_deg: f64,
    /// Circling ends when it has turned less than this within the window.
    pub circling_end_deg: f64,
    /// See `circling_start_deg`.
    pub turn_window_s: f64,
    /// See `circling_start_deg`. Keeps spirals and wing-overs from counting
    /// as thermalling.
    pub circling_min_climb_ms: f64,
    /// Seconds of track kept for drawing behind the glider.
    pub trail_s: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            averaging_s: 20.0,
            takeoff_speed_ms: 4.0,
            takeoff_hold_s: 5.0,
            takeoff_height_m: 25.0,
            landing_speed_ms: 2.0,
            landing_hold_s: 20.0,
            circling_start_deg: 90.0,
            circling_end_deg: 30.0,
            turn_window_s: 30.0,
            circling_min_climb_ms: -0.5,
            trail_s: 180.0,
        }
    }
}

/// One position from a satellite receiver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionSample {
    /// When it was taken, seconds since 1970-01-01 UTC.
    pub time_s: f64,
    /// Where.
    pub position: GeoPoint,
    /// Altitude above sea level in metres, if the receiver gives one.
    pub altitude_m: Option<f64>,
    /// Speed over the ground in metres per second, if the receiver measured
    /// it. Receivers measure it better than it can be worked out from
    /// positions; without it the computer does work it out.
    pub ground_speed_ms: Option<f64>,
    /// Direction of travel in degrees, likewise.
    pub course_deg: Option<f64>,
}

/// Where the flight is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Before take-off.
    Ground,
    /// In the air.
    Flying,
    /// Back on the ground after a flight.
    Landed,
}

/// Where the climb rate comes from, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarioSource {
    /// An external instrument's own vario.
    Instrument,
    /// A pressure sensor, the phone's or an instrument's.
    Pressure,
    /// Satellite altitude: slow and noisy, the last resort.
    Satellite,
}

/// One point of the track behind the glider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailPoint {
    /// When the glider was there, seconds since 1970-01-01 UTC.
    pub time_s: f64,
    /// Where.
    pub position: GeoPoint,
    /// Its altitude in metres.
    pub altitude_m: f64,
    /// Its climb rate there in metres per second.
    pub climb_ms: f64,
}

/// A thermal the glider is circling in, or the last one it left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thermal {
    /// When circling started, seconds since 1970-01-01 UTC.
    pub entered_s: f64,
    /// Altitude at that moment, metres.
    pub entry_altitude_m: f64,
    /// Height gained since, metres.
    pub gain_m: f64,
    /// Average climb since, metres per second.
    pub climb_ms: f64,
}

/// Everything the cockpit shows. A value is `None` until the sensors have
/// supplied what it needs.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveState {
    /// Where the flight is in its life.
    pub phase: Phase,
    /// When the glider took off, seconds since 1970-01-01 UTC.
    pub takeoff_time_s: Option<f64>,
    /// When it landed.
    pub landing_time_s: Option<f64>,
    /// The latest position.
    pub position: Option<GeoPoint>,
    /// The best altitude above sea level, metres: pressure altitude
    /// corrected to agree with the satellites on average, or satellite
    /// altitude alone.
    pub altitude_m: Option<f64>,
    /// Altitude on the standard atmosphere, metres, from the newest pressure.
    pub pressure_altitude_m: Option<f64>,
    /// The climb rate now, metres per second.
    pub climb_ms: Option<f64>,
    /// Where the climb rate comes from.
    pub vario_source: Option<VarioSource>,
    /// The climb rate averaged over the configured time.
    pub average_climb_ms: Option<f64>,
    /// Speed over the ground, metres per second.
    pub ground_speed_ms: Option<f64>,
    /// Direction of travel over the ground, degrees.
    pub course_deg: Option<f64>,
    /// Metres forward per metre of height lost over the last quarter
    /// minute. `None` when not losing height.
    pub glide_ratio: Option<f64>,
    /// The wind, from the last time the glider circled.
    pub wind: Option<Wind>,
    /// Whether the glider is circling in lift.
    pub circling: bool,
    /// The thermal it is in, or the last one it left.
    pub thermal: Option<Thermal>,
    /// Where the glider took off.
    pub takeoff: Option<GeoPoint>,
    /// Height above the take-off, metres.
    pub height_above_takeoff_m: Option<f64>,
    /// Distance to the take-off over the ground, metres.
    pub distance_to_takeoff_m: Option<f64>,
    /// The direction to fly to reach the take-off, degrees.
    pub bearing_to_takeoff_deg: Option<f64>,
    /// The highest altitude of the flight, metres.
    pub max_altitude_m: Option<f64>,
    /// The track of the last few minutes, oldest first.
    pub trail: Vec<TrailPoint>,
}

/// The flight computer. Feed it sensor samples as they arrive, in any mix
/// and at any rate, and ask it for the state of the flight whenever the
/// screen needs it.
#[derive(Debug, Clone)]
pub struct FlightComputer {
    config: Config,

    phase: Phase,
    takeoff: Option<(f64, GeoPoint, f64)>,
    landing_time_s: Option<f64>,
    /// When and where the ground speed first met the take-off or landing
    /// condition in its current unbroken run.
    condition_since: Option<Mark>,
    start_altitude_m: Option<f64>,

    last_position: Option<PositionSample>,
    /// The sample before the latest, as a moment of the flight.
    previous: Option<Mark>,
    velocity: Option<Velocity>,
    /// Whether the latest velocity was worked out from two positions rather
    /// than measured by the receiver.
    velocity_is_derived: bool,
    satellite_altitude_m: Option<f64>,

    pressure_filter: VarioFilter,
    satellite_filter: VarioFilter,
    /// Satellite altitude minus pressure altitude, followed slowly. Adding
    /// it to the pressure altitude gives a height that is as smooth as the
    /// pressure sensor and as true as the satellites.
    pressure_offset_m: Option<f64>,
    instrument_vario: Option<(f64, f64)>,

    /// Recent altitudes for the average climb.
    altitudes: VecDeque<(f64, f64)>,
    /// Recent directions of travel for the circling rule.
    courses: VecDeque<(f64, f64)>,
    circling: bool,
    thermal: Option<Thermal>,
    wind: WindEstimator,
    max_altitude_m: Option<f64>,
    trail: VecDeque<TrailPoint>,
}

/// A moment of the flight: when, where and how high.
#[derive(Debug, Clone, Copy)]
struct Mark {
    time_s: f64,
    position: GeoPoint,
    altitude_m: f64,
}

#[derive(Debug, Clone, Copy)]
struct Velocity {
    speed_ms: f64,
    course_deg: f64,
}

/// How long a source may be silent, in seconds, before the computer stops
/// trusting it and falls back to the next one.
const STALE_S: f64 = 3.0;

impl FlightComputer {
    /// A computer for a new flight.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            phase: Phase::Ground,
            takeoff: None,
            landing_time_s: None,
            condition_since: None,
            start_altitude_m: None,
            last_position: None,
            previous: None,
            velocity: None,
            velocity_is_derived: false,
            satellite_altitude_m: None,
            // A pressure sensor is good to a few decimetres; satellite
            // altitude wanders by metres.
            pressure_filter: VarioFilter::new(0.4, 1.0),
            satellite_filter: VarioFilter::new(4.0, 0.6),
            pressure_offset_m: None,
            instrument_vario: None,
            altitudes: VecDeque::new(),
            courses: VecDeque::new(),
            circling: false,
            thermal: None,
            wind: WindEstimator::new(),
            max_altitude_m: None,
            trail: VecDeque::new(),
        }
    }

    /// Takes a pressure reading in hectopascals.
    pub fn pressure(&mut self, time_s: f64, pressure_hpa: f64) {
        self.pressure_altitude(time_s, pressure_altitude_m(pressure_hpa));
    }

    /// Takes an altitude on the standard atmosphere, from an instrument that
    /// reports that instead of pressure.
    pub fn pressure_altitude(&mut self, time_s: f64, altitude_m: f64) {
        self.pressure_filter.update(time_s, altitude_m);
    }

    /// Forgets the pressure history. Call when a different pressure sensor
    /// takes over: two sensors disagree by a constant, and the step between
    /// them must not be taken for a climb.
    pub fn restart_pressure(&mut self) {
        self.pressure_filter = VarioFilter::new(0.4, 1.0);
        self.pressure_offset_m = None;
    }

    /// Takes a climb rate computed by an external instrument.
    pub fn instrument_vario(&mut self, time_s: f64, climb_ms: f64) {
        self.instrument_vario = Some((time_s, climb_ms));
    }

    /// Takes a position. Positions drive everything that happens once a
    /// second: the trail, the averages, and the take-off, landing and
    /// circling rules.
    pub fn position(&mut self, sample: PositionSample) {
        let now = sample.time_s;
        if self.last_position.is_some_and(|last| now <= last.time_s) {
            return;
        }

        self.velocity_is_derived = sample.ground_speed_ms.is_none() || sample.course_deg.is_none();
        self.velocity = self.velocity_at(&sample).or(self.velocity);
        if let Some(altitude) = sample.altitude_m {
            self.satellite_altitude_m = Some(altitude);
            self.satellite_filter.update(now, altitude);
            if let Some(pressure_altitude) = self.fresh_pressure_altitude(now) {
                let difference = altitude - pressure_altitude;
                // Follow the difference with a time constant of about two
                // minutes: fast enough for the weather, slow enough to
                // average the satellites' wandering away.
                self.pressure_offset_m = Some(match self.pressure_offset_m {
                    None => difference,
                    Some(offset) => offset + (difference - offset) / 120.0,
                });
            }
        }
        self.last_position = Some(sample);

        let altitude = self.altitude(now);
        if let Some(altitude) = altitude {
            self.start_altitude_m.get_or_insert(altitude);
            self.altitudes.push_back((now, altitude));
            while self
                .altitudes
                .front()
                .is_some_and(|&(time, _)| time < now - self.config.averaging_s.max(30.0) - 1.0)
            {
                self.altitudes.pop_front();
            }
        }

        if let Some(velocity) = self.velocity {
            if velocity.speed_ms > 1.0 {
                self.courses.push_back((now, velocity.course_deg));
            }
            while self
                .courses
                .front()
                .is_some_and(|&(time, _)| time < now - self.config.turn_window_s)
            {
                self.courses.pop_front();
            }
            // The sine of a course is its east part, the cosine its north part.
            let (east, north) = velocity.course_deg.to_radians().sin_cos();
            self.wind
                .add(now, velocity.speed_ms * east, velocity.speed_ms * north);
        }

        self.update_phase(now, &sample, altitude);
        self.update_circling(now, altitude);

        if self.phase == Phase::Flying
            && let Some(altitude) = altitude
        {
            self.max_altitude_m = Some(
                self.max_altitude_m
                    .map_or(altitude, |max| max.max(altitude)),
            );
            self.trail.push_back(TrailPoint {
                time_s: now,
                position: sample.position,
                altitude_m: altitude,
                climb_ms: self.climb(now).map_or(0.0, |(climb, _)| climb),
            });
            while self
                .trail
                .front()
                .is_some_and(|point| point.time_s < now - self.config.trail_s)
            {
                self.trail.pop_front();
            }
        }
    }

    /// The state of the flight at `now_s`. Passing the time lets the
    /// computer see which sensors have gone quiet.
    pub fn state(&self, now_s: f64) -> LiveState {
        let altitude = self.altitude(now_s);
        let position = self.last_position.map(|sample| sample.position);
        let climb = self.climb(now_s);
        let takeoff_position = self.takeoff.map(|(_, position, _)| position);

        LiveState {
            phase: self.phase,
            takeoff_time_s: self.takeoff.map(|(time, _, _)| time),
            landing_time_s: self.landing_time_s,
            position,
            altitude_m: altitude,
            pressure_altitude_m: self.fresh_pressure_altitude(now_s),
            climb_ms: climb.map(|(climb, _)| climb),
            vario_source: climb.map(|(_, source)| source),
            average_climb_ms: self.average_climb(self.config.averaging_s),
            ground_speed_ms: self.velocity.map(|velocity| velocity.speed_ms),
            course_deg: self.velocity.map(|velocity| velocity.course_deg),
            glide_ratio: self.glide_ratio(),
            wind: self.wind.wind(),
            circling: self.circling,
            thermal: self.thermal,
            takeoff: takeoff_position,
            height_above_takeoff_m: altitude
                .zip(self.takeoff)
                .map(|(altitude, (_, _, takeoff_altitude))| altitude - takeoff_altitude),
            distance_to_takeoff_m: position
                .zip(takeoff_position)
                .map(|(here, takeoff)| here.distance_m(takeoff)),
            bearing_to_takeoff_deg: position
                .zip(takeoff_position)
                .map(|(here, takeoff)| bearing_deg(here, takeoff)),
            max_altitude_m: self.max_altitude_m,
            trail: self.trail.iter().copied().collect(),
        }
    }

    /// Speed and direction for a new position: the receiver's own if it
    /// gave them, otherwise from the step since the last position.
    fn velocity_at(&self, sample: &PositionSample) -> Option<Velocity> {
        if let (Some(speed_ms), Some(course_deg)) = (sample.ground_speed_ms, sample.course_deg) {
            return Some(Velocity {
                speed_ms,
                course_deg,
            });
        }
        let last = self.last_position?;
        let seconds = sample.time_s - last.time_s;
        if !(0.2..=5.0).contains(&seconds) {
            return None;
        }
        let speed_ms = last.position.distance_m(sample.position) / seconds;
        Some(Velocity {
            speed_ms,
            // Standing still has no direction; keep the last one.
            course_deg: if speed_ms > 0.5 {
                bearing_deg(last.position, sample.position)
            } else {
                self.velocity.map_or(0.0, |velocity| velocity.course_deg)
            },
        })
    }

    fn fresh_pressure_altitude(&self, now_s: f64) -> Option<f64> {
        self.pressure_filter
            .time_s()
            .filter(|&time| now_s - time <= STALE_S)
            .and(self.pressure_filter.altitude_m())
    }

    /// The best altitude above sea level.
    fn altitude(&self, now_s: f64) -> Option<f64> {
        match (self.fresh_pressure_altitude(now_s), self.pressure_offset_m) {
            (Some(pressure_altitude), Some(offset)) => Some(pressure_altitude + offset),
            _ => self
                .satellite_filter
                .altitude_m()
                .or(self.satellite_altitude_m),
        }
    }

    /// The climb rate from the best source that is still talking.
    fn climb(&self, now_s: f64) -> Option<(f64, VarioSource)> {
        if let Some((time, climb)) = self.instrument_vario
            && now_s - time <= STALE_S
        {
            return Some((climb, VarioSource::Instrument));
        }
        if self.fresh_pressure_altitude(now_s).is_some() {
            return self
                .pressure_filter
                .climb_ms()
                .map(|climb| (climb, VarioSource::Pressure));
        }
        self.satellite_filter
            .climb_ms()
            .map(|climb| (climb, VarioSource::Satellite))
    }

    /// Height gained per second over the last `seconds`, from the altitude
    /// history. `None` until at least half that much history exists.
    fn average_climb(&self, seconds: f64) -> Option<f64> {
        let &(now, altitude) = self.altitudes.back()?;
        let &(then, earlier) = self
            .altitudes
            .iter()
            .find(|&&(time, _)| time >= now - seconds)?;
        let span = now - then;
        (span >= seconds / 2.0).then(|| (altitude - earlier) / span)
    }

    fn glide_ratio(&self) -> Option<f64> {
        if self.phase != Phase::Flying {
            return None;
        }
        let latest = self.trail.back()?;
        let earlier = self
            .trail
            .iter()
            .find(|point| point.time_s >= latest.time_s - 15.0)?;
        let lost = earlier.altitude_m - latest.altitude_m;
        if latest.time_s - earlier.time_s < 8.0 || lost < 2.0 {
            return None;
        }
        Some((earlier.position.distance_m(latest.position) / lost).min(99.0))
    }

    /// How far the direction of travel has turned within the window, in
    /// degrees. Turns to the left and to the right cancel, so weaving along
    /// a ridge does not count as circling.
    fn turned_deg(&self) -> f64 {
        let mut total = 0.0;
        let mut courses = self.courses.iter();
        let Some(&(_, mut previous)) = courses.next() else {
            return 0.0;
        };
        for &(_, course) in courses {
            total += (course - previous + 540.0).rem_euclid(360.0) - 180.0;
            previous = course;
        }
        total.abs()
    }

    fn update_circling(&mut self, now: f64, altitude: Option<f64>) {
        let turned = self.turned_deg();
        let window = self.config.turn_window_s;
        if self.phase != Phase::Flying {
            self.circling = false;
        } else if !self.circling {
            let climbing = self
                .average_climb(window)
                .is_some_and(|climb| climb >= self.config.circling_min_climb_ms);
            if turned >= self.config.circling_start_deg && climbing {
                self.circling = true;
                if let Some(altitude) = altitude {
                    self.thermal = Some(Thermal {
                        entered_s: now,
                        entry_altitude_m: altitude,
                        gain_m: 0.0,
                        climb_ms: 0.0,
                    });
                }
            }
        } else if turned < self.config.circling_end_deg
            && self
                .courses
                .front()
                .is_some_and(|&(time, _)| now - time >= window - 2.0)
        {
            self.circling = false;
        }

        if self.circling
            && let (Some(thermal), Some(altitude)) = (self.thermal.as_mut(), altitude)
        {
            thermal.gain_m = altitude - thermal.entry_altitude_m;
            let seconds = now - thermal.entered_s;
            if seconds > 0.0 {
                thermal.climb_ms = thermal.gain_m / seconds;
            }
        }
    }

    fn update_phase(&mut self, now: f64, sample: &PositionSample, altitude: Option<f64>) {
        let speed = self.velocity.map_or(0.0, |velocity| velocity.speed_ms);
        let here = Mark {
            time_s: now,
            position: sample.position,
            altitude_m: altitude.or(sample.altitude_m).unwrap_or(0.0),
        };
        // A speed worked out from two positions describes the step that
        // ended here, so a run it starts began at the position before.
        let run_origin = match self.previous {
            Some(previous) if self.velocity_is_derived => previous,
            _ => here,
        };
        self.previous = Some(here);
        match self.phase {
            Phase::Ground => {
                let fast = speed >= self.config.takeoff_speed_ms;
                if !fast {
                    self.condition_since = None;
                }
                let left_the_ground =
                    altitude
                        .zip(self.start_altitude_m)
                        .is_some_and(|(altitude, start)| {
                            (altitude - start).abs() >= self.config.takeoff_height_m
                        });
                let run_start = fast.then(|| *self.condition_since.get_or_insert(run_origin));
                let held =
                    run_start.is_some_and(|start| now - start.time_s >= self.config.takeoff_hold_s);
                if held || left_the_ground {
                    self.phase = Phase::Flying;
                    self.condition_since = None;
                    // The glider took off when and where its run began, not
                    // where the run was long enough to be believed.
                    let takeoff = run_start.unwrap_or(here);
                    self.takeoff = Some((takeoff.time_s, takeoff.position, takeoff.altitude_m));
                }
            }
            Phase::Flying => {
                let still = speed < self.config.landing_speed_ms
                    && self
                        .average_climb(10.0)
                        .is_none_or(|climb| climb.abs() < 0.5);
                if still {
                    let since = *self.condition_since.get_or_insert(here);
                    if now - since.time_s >= self.config.landing_hold_s {
                        self.phase = Phase::Landed;
                        self.landing_time_s = Some(since.time_s);
                    }
                } else {
                    self.condition_since = None;
                }
            }
            Phase::Landed => {}
        }
    }
}

/// The direction to travel from one point to reach another, in degrees
/// clockwise from north, for the short distances of a flight.
fn bearing_deg(from: GeoPoint, to: GeoPoint) -> f64 {
    let north = (to.lat() - from.lat()) * M_PER_DEG_LAT;
    let east = (to.lon() - from.lon()) * M_PER_DEG_LAT * from.lat().to_radians().cos();
    east.atan2(north).to_degrees().rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::igc::Fix;
    use crate::testing::synthetic_flight;

    /// Replays fixes through a computer the way the app's replay does:
    /// positions with satellite altitude, plus the recorder's pressure
    /// altitude. Returns the state after each fix.
    fn replay(fixes: &[Fix]) -> Vec<LiveState> {
        let mut computer = FlightComputer::new(Config::default());
        fixes
            .iter()
            .map(|fix| {
                let time = fix.time.timestamp() as f64;
                computer.pressure_altitude(time, f64::from(fix.pressure_altitude_m));
                computer.position(PositionSample {
                    time_s: time,
                    position: fix.position,
                    altitude_m: Some(f64::from(fix.gps_altitude_m)),
                    ground_speed_ms: None,
                    course_deg: None,
                });
                computer.state(time)
            })
            .collect()
    }

    // The made-up flight, second by second: a glide out to 90 s, a thermal
    // climbing 2.4 m/s to 342 s, a glide to 722 s, a thermal climbing
    // 1.6 m/s to 890 s, two glides to 1520 s, then a spiral down at 1.9 m/s.

    #[test]
    fn take_off_is_noticed_within_seconds_and_dated_to_when_the_glider_moved() {
        let flight = synthetic_flight();
        let states = replay(&flight.fixes);
        let start = flight.fixes[0].time.timestamp() as f64;

        assert_eq!(states[0].phase, Phase::Ground);
        assert_eq!(states[8].phase, Phase::Flying);
        // The recording starts in flight: the take-off is its first fix.
        assert_eq!(states[8].takeoff_time_s, Some(start));
        assert!(states.last().unwrap().landing_time_s.is_none());
        assert_eq!(states.last().unwrap().phase, Phase::Flying);
    }

    #[test]
    fn the_glides_show_speed_course_sink_and_glide_ratio() {
        let states = replay(&synthetic_flight().fixes);
        // A minute into the first glide: 10.5 m/s on 320 degrees, sinking
        // 1.2 m/s, so a glide ratio near 9.
        let glide = &states[60];
        assert!(
            (glide.ground_speed_ms.unwrap() - 10.5).abs() < 0.4,
            "{glide:?}"
        );
        assert!((glide.course_deg.unwrap() - 320.0).abs() < 3.0);
        assert!((glide.climb_ms.unwrap() + 1.2).abs() < 0.3);
        assert_eq!(glide.vario_source, Some(VarioSource::Pressure));
        assert!((glide.average_climb_ms.unwrap() + 1.2).abs() < 0.2);
        assert!(
            (glide.glide_ratio.unwrap() - 8.75).abs() < 1.0,
            "{:?}",
            glide.glide_ratio
        );
        assert!(!glide.circling);
        assert!(glide.wind.is_none());
    }

    #[test]
    fn a_thermal_is_recognised_measured_and_left() {
        let states = replay(&synthetic_flight().fixes);

        // Circling is recognised within half a minute of the first turn.
        assert!(!states[92].circling);
        assert!(states[125].circling);
        // Deep in the climb: 2.4 m/s, and no glide ratio while climbing.
        let climb = &states[300];
        assert!(climb.circling);
        assert!(
            (climb.climb_ms.unwrap() - 2.4).abs() < 0.3,
            "{:?}",
            climb.climb_ms
        );
        assert_eq!(climb.glide_ratio, None);
        let thermal = climb.thermal.unwrap();
        assert!(thermal.gain_m > 350.0, "{thermal:?}");
        assert!((thermal.climb_ms - 2.4).abs() < 0.2, "{thermal:?}");
        // The made-up thermals drift east at 3 m/s: a westerly.
        let wind = climb.wind.unwrap();
        assert!((wind.from_deg - 270.0).abs() < 8.0, "{wind:?}");
        assert!((wind.speed_ms - 3.0).abs() < 0.5, "{wind:?}");

        // Half a minute into the next glide the circling has ended, and the
        // thermal's totals and the wind are still there to read.
        let after = &states[380];
        assert!(!after.circling);
        assert!(after.thermal.unwrap().gain_m > 550.0);
        assert!(after.wind.is_some());
    }

    #[test]
    fn a_spiral_down_is_not_a_thermal() {
        let states = replay(&synthetic_flight().fixes);
        assert!(states[1600..].iter().all(|state| !state.circling));
        assert!((states[1700].climb_ms.unwrap() + 1.9).abs() < 0.3);
    }

    #[test]
    fn height_and_distance_are_measured_from_the_take_off() {
        let flight = synthetic_flight();
        let states = replay(&flight.fixes);
        let last = states.last().unwrap();

        // Took off at 1620 m, ends at 786 m, topped out at 2117 m.
        assert!(
            (last.height_above_takeoff_m.unwrap() + 834.0).abs() < 15.0,
            "{last:?}"
        );
        assert!((last.max_altitude_m.unwrap() - 2117.0).abs() < 10.0);
        let direct = flight.fixes[0]
            .position
            .distance_m(flight.fixes.last().unwrap().position);
        assert!((last.distance_to_takeoff_m.unwrap() - direct).abs() < 60.0);
        assert!(last.bearing_to_takeoff_deg.is_some());
        // Three minutes of trail at one point a second.
        assert_eq!(last.trail.len(), 181);
    }

    #[test]
    fn the_best_altitude_follows_the_satellites_on_average() {
        // The made-up recorder's pressure altitude reads 35 m low.
        let flight = synthetic_flight();
        let states = replay(&flight.fixes);
        let state = &states[600];
        let truth = f64::from(flight.fixes[600].gps_altitude_m);
        assert!((state.altitude_m.unwrap() - truth).abs() < 3.0);
        assert!((state.pressure_altitude_m.unwrap() - (truth - 35.0)).abs() < 3.0);
    }

    #[test]
    fn without_a_pressure_sensor_the_satellites_give_a_slower_vario() {
        let flight = synthetic_flight();
        let mut computer = FlightComputer::new(Config::default());
        let mut state = None;
        for fix in &flight.fixes[..300] {
            let time = fix.time.timestamp() as f64;
            computer.position(PositionSample {
                time_s: time,
                position: fix.position,
                altitude_m: Some(f64::from(fix.gps_altitude_m)),
                ground_speed_ms: None,
                course_deg: None,
            });
            state = Some(computer.state(time));
        }
        let state = state.unwrap();
        assert_eq!(state.vario_source, Some(VarioSource::Satellite));
        assert!(
            (state.climb_ms.unwrap() - 2.4).abs() < 0.4,
            "{:?}",
            state.climb_ms
        );
    }

    #[test]
    fn an_instrument_vario_wins_while_it_talks_and_is_dropped_when_it_stops() {
        let mut computer = FlightComputer::new(Config::default());
        computer.pressure(100.0, 900.0);
        computer.pressure(100.5, 900.0);
        computer.instrument_vario(100.5, 3.3);

        let state = computer.state(101.0);
        assert_eq!(state.climb_ms, Some(3.3));
        assert_eq!(state.vario_source, Some(VarioSource::Instrument));

        // Five seconds of silence from the instrument, pressure still fresh.
        computer.pressure(105.0, 900.0);
        let state = computer.state(105.5);
        assert_eq!(state.vario_source, Some(VarioSource::Pressure));

        // Everything silent.
        let state = computer.state(120.0);
        assert_eq!(state.climb_ms, None);
        assert_eq!(state.pressure_altitude_m, None);
    }

    #[test]
    fn standing_and_walking_is_not_a_take_off_and_stopping_is_a_landing() {
        let mut computer = FlightComputer::new(Config::default());
        let at =
            |metres_north: f64| GeoPoint::new(47.0 + metres_north / M_PER_DEG_LAT, 11.0).unwrap();
        let mut north = 0.0;
        let mut feed = |computer: &mut FlightComputer, second: i32, speed: f64, altitude: f64| {
            north += speed;
            computer.position(PositionSample {
                time_s: f64::from(second),
                position: at(north),
                altitude_m: Some(altitude),
                ground_speed_ms: None,
                course_deg: None,
            });
        };

        // Two minutes walking around the take-off at 1.5 m/s.
        for second in 0..120 {
            feed(&mut computer, second, 1.5, 1600.0);
        }
        assert_eq!(computer.state(120.0).phase, Phase::Ground);

        // Three minutes of flight at 9 m/s, sinking 1 m/s.
        for second in 120..300 {
            feed(&mut computer, second, 9.0, 1600.0 - f64::from(second - 120));
        }
        let state = computer.state(300.0);
        assert_eq!(state.phase, Phase::Flying);
        // The first fast step is the one from second 119 to second 120.
        assert!((state.takeoff_time_s.unwrap() - 120.0).abs() <= 1.0);

        // Standing in the landing field.
        for second in 300..340 {
            feed(&mut computer, second, 0.0, 1420.0);
        }
        let state = computer.state(340.0);
        assert_eq!(state.phase, Phase::Landed);
        assert!(
            (state.landing_time_s.unwrap() - 302.0).abs() <= 12.0,
            "{state:?}"
        );
    }
}
