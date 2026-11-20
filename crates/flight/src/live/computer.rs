//! The flight computer itself: sensors in, the state of the flight out.

use std::collections::VecDeque;

use venti_domain::GeoPoint;

use super::atmosphere::pressure_altitude_m;
use super::navigation::bearing_deg;
use super::thermal::{self, ThermalPicture};
use super::vario::VarioFilter;
use super::wind::{Wind, WindEstimator, WindFix};

/// The settings of the flight computer.
///
/// The circling rule is the one XCTrack documents, with one addition of
/// our own: once a wind is known, the turn is also measured through the
/// air. The take-off and landing rules are our own. To verify in the air,
/// all of them.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Seconds over which the average climb is taken.
    pub averaging_s: f64,
    /// The glider has taken off once it has moved at least this fast over
    /// the ground, metres per second, for `takeoff_hold_s` seconds, or has
    /// left the height it had `takeoff_height_s` seconds before by
    /// `takeoff_height_m`, steadily. A walking pilot stays under 2 m/s; a
    /// paraglider flies at 8 or more, less into a strong headwind, which is
    /// what the height rule is for. The height is measured over a short
    /// while and not from where the session began, because a pilot who
    /// waits for the take-off walks about the hill: up a slope a walker
    /// gains a third of a metre a second, a glider that has left the ground
    /// a metre or more. Steadily means that each third of the while moved
    /// the same way by a fifth of the height or more: a receiver's first
    /// fixes settle by tens of metres in a step, and a step is not a glider
    /// leaving the ground.
    pub takeoff_speed_ms: f64,
    /// See `takeoff_speed_ms`.
    pub takeoff_hold_s: f64,
    /// See `takeoff_speed_ms`.
    pub takeoff_height_m: f64,
    /// See `takeoff_speed_ms`.
    pub takeoff_height_s: f64,
    /// The glider has landed once it has moved slower than this over the
    /// ground, metres per second, without climbing or sinking, for
    /// `landing_hold_s` seconds.
    pub landing_speed_ms: f64,
    /// See `landing_speed_ms`.
    pub landing_hold_s: f64,
    /// A landing is taken back when the glider flies again, by the rule
    /// for a take-off, within this many seconds of it. A glider parked in
    /// the wind on a ridge or a dune stands still in the air, at one
    /// height, for half a minute and more, and a pilot who touches down
    /// on a dune is off again in a minute: neither has ended the flight.
    /// After this long the landing stands, so that the drive home in a
    /// recorder left running is not flown.
    pub landing_confirm_s: f64,
    /// Circling starts when the direction of travel has turned at least this
    /// many degrees within `turn_window_s` seconds while the average climb
    /// is no worse than `circling_min_climb_ms`. The direction of travel is
    /// the one over the ground or, once a wind is known, the one through
    /// the air, whichever has turned further: in a wind stronger than the
    /// glider is fast the track over the ground swings from side to side
    /// and never comes around.
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
            takeoff_height_s: 30.0,
            landing_speed_ms: 2.0,
            landing_hold_s: 20.0,
            landing_confirm_s: 120.0,
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

/// A thermal the glider has left: where it circled and what it gave. For a
/// map, so that the pilot can find the lift again.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermalMark {
    /// The middle of the circles flown in it. The air drifts with the
    /// wind while the glider climbs, so this lies downwind of where the
    /// thermal leaves the ground.
    pub centre: GeoPoint,
    /// When the glider left it, seconds since 1970-01-01 UTC.
    pub left_s: f64,
    /// The altitude it was left at, metres.
    pub top_altitude_m: f64,
    /// Height gained in it, metres.
    pub gain_m: f64,
    /// Average climb in it, metres per second.
    pub climb_ms: f64,
}

/// The wind as last measured in one band of height. The wind in a valley
/// and the wind above the ridges are two winds; a pilot wants both.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightWind {
    /// The band's floor, metres above sea level. It reaches
    /// [`WIND_BAND_M`] higher.
    pub floor_m: f64,
    /// The wind measured there.
    pub wind: Wind,
    /// When, seconds since 1970-01-01 UTC.
    pub measured_s: f64,
    /// How fast the glider climbed while it circled through the band on
    /// that visit, metres per second: where a thermal is strong and where
    /// it weakens. `None` for a band it circled in for less than ten
    /// seconds.
    pub climb_ms: Option<f64>,
}

/// The height of one band of [`HeightWind`], metres.
pub const WIND_BAND_M: f64 = 200.0;

/// A band's wind is the mean of the measurements of one visit to it. A
/// measurement that comes this many seconds after the last begins a new
/// visit, and the old mean is dropped: the wind has had time to change.
const WIND_VISIT_GAP_S: f64 = 300.0;

/// The measurements of the wind in one band of height during one visit.
#[derive(Debug, Clone, Copy)]
struct WindBand {
    floor_m: f64,
    /// The sums of where the air moved to, east and north, metres per
    /// second, each measurement counted by how far it is trusted, and the
    /// sum of that trust.
    east_ms: f64,
    north_ms: f64,
    count: f64,
    measured_s: f64,
    /// The height gained while circling in the band on this visit, metres,
    /// and the seconds that took.
    gained_m: f64,
    climbing_s: f64,
}

/// A band's climb is given once the glider has circled in it this long,
/// seconds.
const BAND_CLIMB_MIN_S: f64 = 10.0;

impl WindBand {
    fn wind(&self) -> HeightWind {
        let (east, north) = (self.east_ms / self.count, self.north_ms / self.count);
        HeightWind {
            floor_m: self.floor_m,
            wind: Wind {
                // The wind is named by where it comes from.
                from_deg: (-east).atan2(-north).to_degrees().rem_euclid(360.0),
                speed_ms: east.hypot(north),
            },
            measured_s: self.measured_s,
            climb_ms: (self.climbing_s >= BAND_CLIMB_MIN_S)
                .then(|| self.gained_m / self.climbing_s),
        }
    }
}

/// The thermal's picture reaches back this many seconds before circling
/// was noticed: a quarter turn at a thermal's rate, which is what it takes
/// to notice.
const PICTURE_LEAD_S: f64 = 8.0;

/// A thermal is marked when it gave at least this much height, in metres.
/// Less is a bubble, or a turn in lift that led nowhere.
const MARK_MIN_GAIN_M: f64 = 50.0;

/// How many thermals stay marked: the latest ones.
const MARKS_KEPT: usize = 12;

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
    /// When that was, seconds since 1970-01-01 UTC.
    pub wind_measured_s: Option<f64>,
    /// Whether the glider is circling in lift.
    pub circling: bool,
    /// The wind in each band of height the glider has circled in, the
    /// highest band first, each as last measured there.
    pub winds: Vec<HeightWind>,
    /// The thermal it is in, or the last one it left.
    pub thermal: Option<Thermal>,
    /// While circling: the last circles moved with the wind to where their
    /// air is now, and where among them the lift is.
    pub lift: Option<ThermalPicture>,
    /// The thermals it has left that were worth marking, oldest first.
    pub thermals: Vec<ThermalMark>,
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
    /// Recent velocities over the ground for the circling rule: when, and
    /// the east and north parts in metres per second.
    velocities: VecDeque<(f64, f64, f64)>,
    circling: bool,
    thermal: Option<Thermal>,
    /// The sum of the positions flown while circling in the present
    /// thermal, as latitude and longitude, and how many there were.
    circled: (f64, f64, u32),
    thermals: VecDeque<ThermalMark>,
    wind: WindEstimator,
    /// When the wind was last measured.
    wind_measured_s: Option<f64>,
    /// The last moment the glider was known to be turning, and its
    /// altitude then: where the climb of a band of height was counted to.
    band_mark: Option<(f64, f64)>,
    /// The wind by band of height, the highest band first.
    winds: Vec<WindBand>,
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

/// A step between two positions that no glider makes: more than this many
/// metres, and faster than [`JUMP_MS`]. The receiver has found out where
/// it is, or the phone was carried while the receiver slept.
const JUMP_M: f64 = 500.0;
/// See [`JUMP_M`]. Metres per second: 360 km/h.
const JUMP_MS: f64 = 100.0;

/// How old the latest position may be, in seconds, before what is worked
/// out from positions is no longer reported: speed, course, glide, and
/// the altitude and climb where they come from the satellites. A receiver
/// reports once a second; five seconds without is a fix that is lost, and
/// an old number shown as if it were now is worse than none.
const POSITION_STALE_S: f64 = 5.0;

impl FlightComputer {
    /// A computer for a new flight.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            phase: Phase::Ground,
            takeoff: None,
            landing_time_s: None,
            condition_since: None,
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
            velocities: VecDeque::new(),
            circling: false,
            thermal: None,
            circled: (0.0, 0.0, 0),
            thermals: VecDeque::new(),
            winds: Vec::new(),
            wind: WindEstimator::new(),
            wind_measured_s: None,
            band_mark: None,
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
        // On the ground a step no glider makes is the receiver finding
        // out where it is: what was learnt before it is forgotten, so
        // that the step is taken neither for speed nor for height.
        if self.phase == Phase::Ground
            && let Some(last) = self.last_position
            && last.position.distance_m(sample.position) > JUMP_M.max(JUMP_MS * (now - last.time_s))
        {
            self.forget_the_ground();
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
            self.altitudes.push_back((now, altitude));
            let kept_s = self
                .config
                .averaging_s
                .max(self.config.takeoff_height_s)
                .max(30.0);
            while self
                .altitudes
                .front()
                .is_some_and(|&(time, _)| time < now - kept_s - 1.0)
            {
                self.altitudes.pop_front();
            }
        }

        if let Some(velocity) = self.velocity {
            // The sine of a course is its east part, the cosine its north part.
            let (east, north) = velocity.course_deg.to_radians().sin_cos();
            let (east, north) = (velocity.speed_ms * east, velocity.speed_ms * north);
            self.velocities.push_back((now, east, north));
            while self
                .velocities
                .front()
                .is_some_and(|&(time, _, _)| time < now - self.config.turn_window_s)
            {
                self.velocities.pop_front();
            }
            let measured = self.wind.add(now, east, north);
            if measured.is_some() {
                self.wind_measured_s = Some(now);
            }
            if let (Some(fix), Some(altitude)) = (measured, altitude) {
                self.note_wind(now, altitude, fix);
            }
        }

        self.update_phase(now, &sample, altitude);
        self.update_circling(now, altitude);

        // The climb of the band of height the glider turns in, counted
        // into the band's present visit: from one moment the glider is
        // known to be turning to the next, which runs two seconds behind
        // it. The circling rule would not do: it lets go half a minute
        // after the last circle, and the first of the glide's sink would
        // count as the thermal's top. A band is there once a wind was
        // measured in it, which while circling is within a circle.
        let turning = self.wind.turning_until().and_then(|until| {
            self.altitudes
                .iter()
                .rev()
                .find(|&&(time, _)| time <= until)
                .copied()
        });
        if let (Some((time, altitude)), Some((since, from))) = (turning, self.band_mark) {
            let seconds = time - since;
            let floor_m = (altitude / WIND_BAND_M).floor() * WIND_BAND_M;
            if seconds > 0.0
                && seconds <= STALE_S
                && let Ok(index) = self
                    .winds
                    .binary_search_by(|kept| floor_m.total_cmp(&kept.floor_m))
                && now - self.winds[index].measured_s <= WIND_VISIT_GAP_S
            {
                let band = &mut self.winds[index];
                band.gained_m += altitude - from;
                band.climbing_s += seconds;
            }
        }
        self.band_mark = turning;

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
        // What is worked out from positions says nothing once they stop.
        let velocity = self.velocity.filter(|_| self.has_position(now_s));

        LiveState {
            phase: self.phase,
            takeoff_time_s: self.takeoff.map(|(time, _, _)| time),
            landing_time_s: self.landing_time_s,
            position,
            altitude_m: altitude,
            pressure_altitude_m: self.fresh_pressure_altitude(now_s),
            climb_ms: climb.map(|(climb, _)| climb),
            vario_source: climb.map(|(_, source)| source),
            average_climb_ms: self
                .average_climb(self.config.averaging_s)
                .filter(|_| self.has_position(now_s)),
            ground_speed_ms: velocity.map(|velocity| velocity.speed_ms),
            course_deg: velocity.map(|velocity| velocity.course_deg),
            glide_ratio: self.glide_ratio().filter(|_| self.has_position(now_s)),
            wind: self.wind.wind(),
            wind_measured_s: self.wind_measured_s,
            winds: self.winds.iter().map(WindBand::wind).collect(),
            circling: self.circling,
            thermal: self.thermal,
            lift: self.thermal.filter(|_| self.circling).and_then(|thermal| {
                thermal::picture(
                    self.trail.iter().copied(),
                    thermal.entered_s - PICTURE_LEAD_S,
                    now_s,
                    self.wind.wind(),
                )
            }),
            thermals: self.thermals.iter().copied().collect(),
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

    /// Forgets what the positions on the ground have taught: where the
    /// glider was, how fast, how high.
    fn forget_the_ground(&mut self) {
        self.condition_since = None;
        self.previous = None;
        self.last_position = None;
        self.velocity = None;
        self.altitudes.clear();
        self.velocities.clear();
        self.satellite_filter = VarioFilter::new(4.0, 0.6);
        self.satellite_altitude_m = None;
        self.pressure_offset_m = None;
        self.trail.clear();
    }

    /// Whether a position has arrived lately enough to speak for now.
    fn has_position(&self, now_s: f64) -> bool {
        self.last_position
            .is_some_and(|sample| now_s - sample.time_s <= POSITION_STALE_S)
    }

    /// The best altitude above sea level.
    fn altitude(&self, now_s: f64) -> Option<f64> {
        match (self.fresh_pressure_altitude(now_s), self.pressure_offset_m) {
            (Some(pressure_altitude), Some(offset)) => Some(pressure_altitude + offset),
            _ => self
                .satellite_filter
                .altitude_m()
                .or(self.satellite_altitude_m)
                .filter(|_| self.has_position(now_s)),
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
            .filter(|_| self.has_position(now_s))
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
    ///
    /// It is measured twice, over the ground and, with the wind last
    /// measured taken off, through the air, and the larger counts. Flying
    /// straight turns neither, whatever wind is taken off. Circling turns
    /// both in a light wind and only the second in a strong one. A wind
    /// that was measured wrong, or lower down, can spoil the second for a
    /// circle, and then the first still stands.
    fn turned_deg(&self) -> f64 {
        let turned = |drift: (f64, f64)| {
            let mut total = 0.0;
            let mut previous: Option<f64> = None;
            for &(_, east, north) in &self.velocities {
                let (east, north) = (east - drift.0, north - drift.1);
                // A glider barely moving has no direction worth the name.
                if east.hypot(north) <= 1.0 {
                    continue;
                }
                let course = east.atan2(north).to_degrees();
                if let Some(previous) = previous {
                    total += (course - previous + 540.0).rem_euclid(360.0) - 180.0;
                }
                previous = Some(course);
            }
            total.abs()
        };
        let over_the_ground = turned((0.0, 0.0));
        match self.wind.wind() {
            Some(wind) => {
                let (sin, cos) = wind.from_deg.to_radians().sin_cos();
                over_the_ground.max(turned((-wind.speed_ms * sin, -wind.speed_ms * cos)))
            }
            None => over_the_ground,
        }
    }

    fn update_circling(&mut self, now: f64, altitude: Option<f64>) {
        let turned = self.turned_deg();
        let window = self.config.turn_window_s;
        let was_circling = self.circling;
        if self.phase != Phase::Flying {
            self.circling = false;
        } else if !self.circling {
            let climbing = self
                .average_climb(window)
                .is_some_and(|climb| climb >= self.config.circling_min_climb_ms);
            if turned >= self.config.circling_start_deg && climbing {
                self.circling = true;
                self.circled = (0.0, 0.0, 0);
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
                .velocities
                .front()
                .is_some_and(|&(time, _, _)| now - time >= window - 2.0)
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
        if self.circling
            && let Some(sample) = self.last_position
        {
            self.circled.0 += sample.position.lat();
            self.circled.1 += sample.position.lon();
            self.circled.2 += 1;
        }
        if was_circling && !self.circling {
            self.mark_thermal(now, altitude);
        }
    }

    /// Takes a fresh measurement of the wind into the mean of the band of
    /// height the glider is in. One measurement is a fit to the last
    /// circle, and the first and the last of a climb are fits to a circle
    /// that is only just one: the mean over the visit is steadier than any
    /// of them. Each counts by how well its circle fitted, so a circle
    /// flown through rough air moves the mean less than a clean one.
    fn note_wind(&mut self, now: f64, altitude: f64, fix: WindFix) {
        let floor_m = (altitude / WIND_BAND_M).floor() * WIND_BAND_M;
        let (sin, cos) = fix.wind.from_deg.to_radians().sin_cos();
        let (east, north) = (
            -fix.wind.speed_ms * sin * fix.quality,
            -fix.wind.speed_ms * cos * fix.quality,
        );
        let fresh = WindBand {
            floor_m,
            east_ms: east,
            north_ms: north,
            count: fix.quality,
            measured_s: now,
            gained_m: 0.0,
            climbing_s: 0.0,
        };
        match self
            .winds
            .binary_search_by(|kept| floor_m.total_cmp(&kept.floor_m))
        {
            Ok(index) if now - self.winds[index].measured_s <= WIND_VISIT_GAP_S => {
                let band = &mut self.winds[index];
                band.east_ms += east;
                band.north_ms += north;
                band.count += fix.quality;
                band.measured_s = now;
            }
            Ok(index) => self.winds[index] = fresh,
            Err(index) => self.winds.insert(index, fresh),
        }
    }

    /// Marks the thermal just left, if it was worth it.
    fn mark_thermal(&mut self, now: f64, altitude: Option<f64>) {
        let (lat, lon, positions) = self.circled;
        let (Some(thermal), Some(altitude)) = (self.thermal, altitude) else {
            return;
        };
        if thermal.gain_m < MARK_MIN_GAIN_M || positions == 0 {
            return;
        }
        let count = f64::from(positions);
        let Ok(centre) = GeoPoint::new(lat / count, lon / count) else {
            return;
        };
        if self.thermals.len() == MARKS_KEPT {
            self.thermals.pop_front();
        }
        self.thermals.push_back(ThermalMark {
            centre,
            left_s: now,
            top_altitude_m: altitude,
            gain_m: thermal.gain_m,
            climb_ms: thermal.climb_ms,
        });
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
                if let Some(takeoff) = self.takes_off(now, speed, here, run_origin, altitude) {
                    self.phase = Phase::Flying;
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
                        self.condition_since = None;
                    }
                } else {
                    self.condition_since = None;
                }
            }
            Phase::Landed => {
                // Flying again soon after: the glider had not landed, or
                // not for good. The flight goes on from its one take-off.
                let recent = self
                    .landing_time_s
                    .is_some_and(|landed| now - landed <= self.config.landing_confirm_s);
                if recent
                    && self
                        .takes_off(now, speed, here, run_origin, altitude)
                        .is_some()
                {
                    self.phase = Phase::Flying;
                    self.landing_time_s = None;
                }
            }
        }
    }

    /// Whether the glider has just left the ground, by the rule for a
    /// take-off, and if so the moment and place its run began.
    fn takes_off(
        &mut self,
        now: f64,
        speed: f64,
        here: Mark,
        run_origin: Mark,
        altitude: Option<f64>,
    ) -> Option<Mark> {
        let fast = speed >= self.config.takeoff_speed_ms;
        if !fast {
            self.condition_since = None;
        }
        // Against the height of a short while ago, in thirds of that
        // while: the newest altitude that is at least so old.
        let third = self.config.takeoff_height_s / 3.0;
        let ago = |thirds: f64| {
            self.altitudes
                .iter()
                .rev()
                .find(|&&(time, _)| time <= now - thirds * third)
                .map(|&(_, altitude)| altitude)
        };
        let left_the_ground = match (altitude, ago(1.0), ago(2.0), ago(3.0)) {
            (Some(here), Some(one), Some(two), Some(three)) => {
                let steps = [two - three, one - two, here - one];
                let steady = steps.iter().all(|step| {
                    step * steps[0] > 0.0 && step.abs() >= self.config.takeoff_height_m / 5.0
                });
                steady && (here - three).abs() >= self.config.takeoff_height_m
            }
            _ => false,
        };
        let run_start = fast.then(|| *self.condition_since.get_or_insert(run_origin));
        let held = run_start.is_some_and(|start| now - start.time_s >= self.config.takeoff_hold_s);
        if held || left_the_ground {
            self.condition_since = None;
            // The glider took off when and where its run began, not where
            // the run was long enough to be believed.
            Some(run_start.unwrap_or(here))
        } else {
            None
        }
    }

    /// Since when the glider, in flight, has been as still as a landed one:
    /// slow over the ground and at one height. `None` on the ground, once
    /// landed, and while it moves. A recording that ends in such a moment
    /// ends with a landing the recorder did not wait to see through.
    pub fn at_rest_since(&self) -> Option<f64> {
        match self.phase {
            Phase::Flying => self.condition_since.map(|since| since.time_s),
            Phase::Ground | Phase::Landed => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::igc::Fix;

    /// Metres per degree of latitude on the FAI sphere.
    const M_PER_DEG_LAT: f64 = 111_194.9;
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
        let later = after.wind.unwrap();
        assert!((later.from_deg - 270.0).abs() < 8.0, "{later:?}");
        assert!((later.speed_ms - 3.0).abs() < 0.5, "{later:?}");
        // Measured while circling, the last time a few seconds after the
        // circles ended at second 342, which is how long it takes to see
        // that they have.
        let start = synthetic_flight().fixes[0].time.timestamp() as f64;
        let measured = after.wind_measured_s.unwrap() - start;
        assert!((336.0..=350.0).contains(&measured), "{measured}");
        assert!(states[300].wind_measured_s.unwrap() - start > 290.0);
        // The wind was measured in every band of height the climb went
        // through, the highest first, and each says the same westerly.
        assert!(after.winds.len() >= 3, "{:?}", after.winds);
        assert!(
            after
                .winds
                .windows(2)
                .all(|pair| pair[0].floor_m > pair[1].floor_m)
        );
        let top = after.winds[0];
        assert!(top.floor_m <= after.max_altitude_m.unwrap());
        assert!(top.floor_m + WIND_BAND_M > after.max_altitude_m.unwrap() - 60.0);
        for band in &after.winds {
            assert!((band.wind.from_deg - 270.0).abs() < 12.0, "{band:?}");
            assert!((band.wind.speed_ms - 3.0).abs() < 0.8, "{band:?}");
            assert_eq!(band.floor_m % WIND_BAND_M, 0.0);
        }
        // And how the thermal climbed in each: 2.4 m/s all the way up,
        // which takes more than a minute for a band. The lowest band was
        // entered near its ceiling and the highest left near its floor;
        // those say what they can or nothing.
        let climbs: Vec<f64> = after
            .winds
            .iter()
            .filter_map(|band| band.climb_ms)
            .collect();
        assert!(climbs.len() >= 2, "{:?}", after.winds);
        for climb in climbs {
            assert!((climb - 2.4).abs() < 0.3, "{climb}");
        }
        assert!(states[60].winds.is_empty());

        // The thermal is marked where the glider circled, with what it gave.
        assert_eq!(after.thermals.len(), 1);
        let mark = after.thermals[0];
        assert!(mark.gain_m > 550.0, "{mark:?}");
        // Averaged to the moment the circling rule lets go, some seconds
        // into the glide: a little under the 2.4 m/s of the climb itself.
        assert!((1.9..2.5).contains(&mark.climb_ms), "{mark:?}");
        let entered = states[125].position.unwrap();
        let left = states[342].position.unwrap();
        assert!(mark.centre.distance_m(entered) < entered.distance_m(left) + 150.0);
        assert!(mark.centre.distance_m(after.position.unwrap()) < 1500.0);
        assert!(
            states[330].thermals.is_empty(),
            "marked only once it is left"
        );

        // And it holds through the long glide, until the next thermal.
        let before_next = states[700].wind.unwrap();
        assert!((before_next.speed_ms - 3.0).abs() < 0.5, "{before_next:?}");
    }

    #[test]
    fn the_thermal_is_pictured_as_the_circles_flown_in_its_moving_air() {
        let states = replay(&synthetic_flight().fixes);
        // On the glide there is nothing to picture.
        assert!(states[60].lift.is_none());

        // Deep in the climb. Over the ground the last fifty seconds are
        // loops stretched 150 m by the wind; moved with the wind they are
        // the circles of 45 m the glider flew.
        let lift = states[300].lift.as_ref().unwrap();
        assert!(lift.drifted);
        assert_eq!(lift.points.len(), 51);
        let centre = lift.centre.unwrap();
        assert!((centre.radius_m - 45.0).abs() < 3.0, "{centre:?}");
        for point in &lift.points {
            let distance = point.position.distance_m(centre.circle);
            assert!((distance - 45.0).abs() < 12.0, "{point:?}");
        }
        // The made-up thermal lifts evenly: the lift is where the circle is.
        assert!((centre.climb_ms - 2.4).abs() < 0.2, "{centre:?}");
        assert!(centre.shift_m < 0.3 * centre.radius_m, "{centre:?}");
        // The newest place is the glider itself.
        let newest = lift.points.last().unwrap();
        assert_eq!(newest.age_s, 0.0);
        assert!(newest.position.distance_m(states[300].position.unwrap()) < 0.5);

        // Gone once the thermal is left.
        assert!(states[380].lift.is_none());
    }

    #[test]
    fn circling_is_recognised_in_a_wind_stronger_than_the_glider_is_fast() {
        // A glider flying 10 m/s in 12 m/s from the north: a glide downwind
        // for half a minute, then circles of 28 s climbing 2 m/s. Over the
        // ground it is carried south the whole time, backwards on the
        // upwind half of each circle, and its track only swings between
        // south-east and south-west.
        let mut computer = FlightComputer::new(Config::default());
        let (mut east, mut north) = (0.0_f64, 0.0_f64);
        let mut states = Vec::new();
        for second in 0..200 {
            let turning_for = f64::from((second - 30).max(0));
            let heading = 180.0_f64.to_radians() + std::f64::consts::TAU * turning_for / 28.0;
            east += 10.0 * heading.sin();
            north += 10.0 * heading.cos() - 12.0;
            let altitude = 1500.0
                + if second < 30 {
                    -f64::from(second)
                } else {
                    -30.0 + 2.0 * turning_for
                };
            let time = f64::from(second);
            computer.pressure_altitude(time, altitude);
            computer.position(PositionSample {
                time_s: time,
                position: GeoPoint::new(
                    47.0 + north / M_PER_DEG_LAT,
                    11.0 + east / (M_PER_DEG_LAT * 47.0_f64.to_radians().cos()),
                )
                .unwrap(),
                altitude_m: Some(altitude),
                ground_speed_ms: None,
                course_deg: None,
            });
            states.push(computer.state(time));
        }

        // Recognised once the first circle has given the wind, and held
        // from then on: the track over the ground alone would say
        // "circling" for a few seconds of each circle and then let go.
        assert!(!states[35].circling);
        assert!(states[70..].iter().all(|state| state.circling));
        let wind = states[120].wind.unwrap();
        assert!(wind.from_deg < 3.0 || wind.from_deg > 357.0, "{wind:?}");
        assert!((wind.speed_ms - 12.0).abs() < 0.3, "{wind:?}");
        // And the thermal's picture is the circle flown: ten metres per
        // second around in 28 s is a radius of 44.6 m.
        let centre = states[120].lift.as_ref().unwrap().centre.unwrap();
        assert!((centre.radius_m - 44.6).abs() < 2.0, "{centre:?}");
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

    #[test]
    fn a_glider_parked_in_the_wind_has_landed_only_if_it_stays_down() {
        let fly =
            |computer: &mut FlightComputer, seconds: std::ops::Range<i32>, north: &mut f64| {
                for second in seconds {
                    *north += 9.0;
                    step(computer, second, *north, 300.0);
                }
            };
        let stand = |computer: &mut FlightComputer, seconds: std::ops::Range<i32>, north: f64| {
            for second in seconds {
                step(computer, second, north, 300.0);
            }
        };

        // Soaring a dune at one height, then standing in the wind for
        // forty seconds: by the rule for a landing, landed.
        let mut computer = FlightComputer::new(Config::default());
        let mut north = 0.0;
        fly(&mut computer, 0..120, &mut north);
        stand(&mut computer, 120..160, north);
        let parked = computer.state(160.0);
        assert_eq!(parked.phase, Phase::Landed);
        assert!(
            (parked.landing_time_s.unwrap() - 121.0).abs() <= 3.0,
            "{parked:?}"
        );

        // It flies on: the landing is taken back, and the flight is the
        // one that began at its take-off.
        fly(&mut computer, 160..200, &mut north);
        let on = computer.state(200.0);
        assert_eq!(on.phase, Phase::Flying);
        assert_eq!(on.landing_time_s, None);
        assert_eq!(on.takeoff_time_s, parked.takeoff_time_s);

        // Down for good: once two minutes have passed, moving off fast is
        // no flight, whatever it is.
        stand(&mut computer, 200..360, north);
        let landed = computer.state(360.0);
        assert_eq!(landed.phase, Phase::Landed);
        fly(&mut computer, 360..420, &mut north);
        let driven = computer.state(420.0);
        assert_eq!(driven.phase, Phase::Landed);
        assert_eq!(driven.landing_time_s, landed.landing_time_s);
    }

    #[test]
    fn a_flying_glider_says_since_when_it_has_been_at_rest() {
        let mut computer = FlightComputer::new(Config::default());
        let mut north = 0.0;
        for second in 0..60 {
            north += 9.0;
            step(&mut computer, second, north, 300.0);
        }
        assert_eq!(computer.at_rest_since(), None);
        for second in 60..70 {
            step(&mut computer, second, north, 300.0);
        }
        // Not yet landed by the cockpit's rule, and at rest for some
        // seconds: what the end of a recording looks like.
        assert_eq!(computer.state(70.0).phase, Phase::Flying);
        let since = computer.at_rest_since().unwrap();
        assert!((60.0..=64.0).contains(&since), "{since}");
    }

    /// Feeds a second of standing, walking or flying straight north.
    fn step(computer: &mut FlightComputer, second: i32, north_m: f64, altitude_m: f64) {
        computer.position(PositionSample {
            time_s: f64::from(second),
            position: GeoPoint::new(47.0 + north_m / M_PER_DEG_LAT, 11.0).unwrap(),
            altitude_m: Some(altitude_m),
            ground_speed_ms: None,
            course_deg: None,
        });
    }

    #[test]
    fn what_positions_gave_is_not_reported_once_they_stop() {
        let mut computer = FlightComputer::new(Config::default());
        // A glide without a pressure sensor, then the fix is lost.
        for second in 0..60 {
            let flown = f64::from(second);
            step(&mut computer, second, 10.0 * flown, 1600.0 - flown);
        }
        let state = computer.state(60.0);
        assert!(state.ground_speed_ms.is_some());
        assert!(state.glide_ratio.is_some());
        assert!(state.climb_ms.is_some());
        assert!(state.altitude_m.is_some());

        let state = computer.state(70.0);
        assert_eq!(state.ground_speed_ms, None);
        assert_eq!(state.course_deg, None);
        assert_eq!(state.glide_ratio, None);
        assert_eq!(state.climb_ms, None);
        assert_eq!(state.average_climb_ms, None);
        assert_eq!(state.altitude_m, None);
        // Where the glider last was, and that it flies, stay known.
        assert!(state.position.is_some());
        assert_eq!(state.phase, Phase::Flying);
    }

    #[test]
    fn a_receiver_that_settles_in_a_step_has_not_taken_off() {
        let mut computer = FlightComputer::new(Config::default());
        // Standing at the take-off. The first fixes are fifty metres too
        // high, as first fixes are; after ten seconds they are right.
        for second in 0..600 {
            let altitude = if second < 10 { 1650.0 } else { 1600.0 };
            step(&mut computer, second, 0.0, altitude);
            assert_eq!(
                computer.state(f64::from(second)).phase,
                Phase::Ground,
                "second {second}"
            );
        }

        // Nor when the receiver finds out that it is somewhere else
        // altogether, some seconds after its first word.
        let mut computer = FlightComputer::new(Config::default());
        for second in 0..5 {
            step(&mut computer, second, 0.0, 30.0);
        }
        for second in 5..300 {
            step(&mut computer, second, 800_000.0, 1600.0);
            assert_eq!(
                computer.state(f64::from(second)).phase,
                Phase::Ground,
                "second {second}"
            );
        }
        // And from there a flight is a flight.
        for second in 300..330 {
            let flown = f64::from(second - 300);
            step(
                &mut computer,
                second,
                800_000.0 + 9.0 * flown,
                1600.0 - flown,
            );
        }
        assert_eq!(computer.state(330.0).phase, Phase::Flying);
    }

    #[test]
    fn walking_up_to_the_take_off_is_not_a_take_off() {
        let mut computer = FlightComputer::new(Config::default());
        // Ten minutes up a steep path: 1.2 m/s along it, a third of a metre
        // a second up, 180 m of height in all.
        for second in 0..600 {
            let seconds = f64::from(second);
            step(&mut computer, second, 1.2 * seconds, 1400.0 + 0.3 * seconds);
        }
        let state = computer.state(600.0);
        assert_eq!(state.phase, Phase::Ground, "{state:?}");
        assert!(state.takeoff_time_s.is_none());
    }

    #[test]
    fn a_glider_hanging_in_a_headwind_has_taken_off_by_its_height() {
        let mut computer = FlightComputer::new(Config::default());
        // Five minutes at the take-off, then into a wind as fast as the
        // glider: 1 m/s over the ground, sinking 1.2 m/s.
        for second in 0..300 {
            step(&mut computer, second, 0.0, 1600.0);
        }
        assert_eq!(computer.state(300.0).phase, Phase::Ground);
        for second in 300..340 {
            let flown = f64::from(second - 300);
            step(&mut computer, second, flown, 1600.0 - 1.2 * flown);
        }
        let state = computer.state(340.0);
        assert_eq!(state.phase, Phase::Flying, "{state:?}");
    }
}
