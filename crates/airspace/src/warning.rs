//! The airspace warnings of the cockpit.
//!
//! [`WarningEngine`] is fed one [`Sample`] after another, about once a
//! second, and answers each with the warnings that hold at that moment: at
//! most one per airspace, the most urgent first. Like the flight computer it
//! reads no clock. Every sample carries its own time, so a recorded flight
//! replayed through it produces exactly the warnings the pilot got.
//!
//! # What a warning says
//!
//! | Level | Meaning |
//! |---|---|
//! | [`Level::Inside`] | In the airspace, or possibly in it: see [`Warning::uncertain`] |
//! | [`Level::Entering`] | Will be in it within the look-ahead time if course and climb stay as they are |
//! | [`Level::VeryNear`] | Within the inner margins |
//! | [`Level::Near`] | Within the outer margins |
//!
//! The margins default to what XCTrack uses: "really close" at 100 m of
//! height and "close" at 500 m, with distance over the ground counting a
//! quarter as much, which makes 400 m and 2000 m. XCTrack blends the two
//! into one scaled distance by a formula its manual does not give; this
//! engine warns when both are within their margin, which is never later.
//!
//! # Stable, not flickering
//!
//! A pilot circling on a boundary must not get a warning that comes and
//! goes with every turn. A warning rises to a more urgent level at once. It
//! falls back only when the pilot is beyond the level's margin by an extra
//! distance ([`Config::hysteresis`]) and has been for a few seconds
//! ([`Config::hold_s`]). Leaving an airspace by ten metres therefore still
//! reads "inside" for a moment: wrong on the side of caution.
//!
//! # Where the pilot is heading
//!
//! The prediction does not use the course of the last second, which sweeps
//! the whole compass in a thermal. It uses the path over the last twenty
//! seconds: in straight flight that is the course, and in a thermal it is
//! the drift, which is what carries a circling pilot into airspace.

use std::collections::{BTreeMap, VecDeque};

use venti_domain::{Airspace, AirspaceId, GeoPoint};

use crate::geometry::{bearing_deg, destination, inside_stretches};
use crate::index::{AirspaceIndex, Horizontal, horizontal};
use crate::policy::{Policy, PolicyTable};
use crate::vertical::{Altitudes, PressureAltitude, VerticalStatus, vertical_status};

/// What the cockpit knows at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// When, in seconds since 1970-01-01 UTC, as the flight computer counts.
    pub time_s: f64,
    /// Where.
    pub position: GeoPoint,
    /// Altitude above mean sea level in metres: the cockpit's best altitude.
    pub altitude_m: f64,
    /// The pressure altitude, if a pressure sensor is there. Without it
    /// flight levels are placed with a wide margin.
    pub pressure: Option<PressureAltitude>,
    /// Elevation of the ground below, metres above mean sea level, if known.
    /// Without it limits above ground are taken to reach the pilot.
    pub ground_elevation_m: Option<f64>,
    /// Speed over the ground in metres per second, if known.
    pub ground_speed_ms: Option<f64>,
    /// Direction of travel in degrees, if known.
    pub course_deg: Option<f64>,
    /// Climb rate in metres per second, negative when sinking, if known.
    /// The averaged climb predicts better than the momentary one.
    pub climb_ms: Option<f64>,
}

impl Sample {
    /// The three altitudes of the sample, as the queries take them.
    pub fn altitudes(&self) -> Altitudes {
        Altitudes {
            altitude_m: self.altitude_m,
            pressure: self.pressure,
            ground_elevation_m: self.ground_elevation_m,
        }
    }
}

/// A sample the engine cannot work with.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum SampleError {
    /// The time is not a finite number.
    #[error("the sample's time is not a number")]
    Time,
    /// The altitude is not a finite number.
    #[error("the sample's altitude is not a number")]
    Altitude,
}

/// A distance over the ground and a height, in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margin {
    /// Over the ground.
    pub horizontal_m: f64,
    /// In height.
    pub vertical_m: f64,
}

/// The settings of the warning engine.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The pilot is near an airspace when closer than this both ways.
    /// XCTrack's "airspace is close": 500 m of height, 2000 m over the ground.
    pub near: Margin,
    /// The pilot is very near when closer than this both ways. XCTrack's
    /// "airspace is really close": 100 m of height, 400 m over the ground.
    pub very_near: Margin,
    /// How far beyond a margin, or outside an airspace, the pilot must be
    /// before a warning falls back to a lower level.
    pub hysteresis: Margin,
    /// How long the conditions for a lower level must have held before a
    /// warning falls back to it, seconds.
    pub hold_s: f64,
    /// How far ahead entry is predicted, seconds. Our own value: XCTrack
    /// documents no prediction for airspace. To verify in the air.
    pub look_ahead_s: f64,
    /// The stretch of flight the direction of travel is taken from, seconds.
    pub velocity_window_s: f64,
    /// Which airspaces are warned about.
    pub policy: PolicyTable,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            near: Margin {
                horizontal_m: 2000.0,
                vertical_m: 500.0,
            },
            very_near: Margin {
                horizontal_m: 400.0,
                vertical_m: 100.0,
            },
            hysteresis: Margin {
                horizontal_m: 50.0,
                vertical_m: 20.0,
            },
            hold_s: 10.0,
            look_ahead_s: 60.0,
            velocity_window_s: 20.0,
            policy: PolicyTable::paraglider(),
        }
    }
}

/// How urgent a warning is, the least urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Level {
    /// Within the outer margins.
    Near,
    /// Within the inner margins.
    VeryNear,
    /// Will be inside within the look-ahead time.
    Entering,
    /// Inside, or possibly inside.
    Inside,
}

/// The way a predicted entry happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryVia {
    /// Flying in through the side.
    Side,
    /// Climbing in through the floor.
    Floor,
    /// Sinking in through the ceiling.
    Ceiling,
}

/// A predicted entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// Seconds until the pilot is inside if course and climb stay as they are.
    pub seconds: f64,
    /// Which way in.
    pub via: EntryVia,
}

/// How long the pilot wants a warning or an airspace out of the way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Silence {
    /// For this many seconds from the latest sample.
    For {
        /// How many seconds.
        seconds: f64,
    },
    /// Until this time, in seconds since 1970-01-01 UTC.
    Until {
        /// The time.
        time_s: f64,
    },
    /// Until the day ends where the pilot is.
    RestOfDay {
        /// The local time zone's offset from UTC in seconds, east positive:
        /// 3600 in Germany and Austria in winter, 7200 in summer.
        utc_offset_s: i32,
    },
}

/// Why a warning is quiet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Silenced {
    /// The pilot acknowledged the warning at this level or a more urgent
    /// one. It speaks again if it becomes more urgent than it was then.
    Acknowledged,
    /// The pilot said this airspace does not apply for now.
    Ignored,
}

/// One warning: the state of the pilot in relation to one airspace.
///
/// There is at most one warning per airspace, and the airspace's identifier
/// is the warning's identity from one answer to the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Warning<'a> {
    /// The airspace. Its record has the name, the published limits, the
    /// source and the age of the data, all of which belong on the screen.
    pub airspace: &'a Airspace,
    /// How urgent.
    pub level: Level,
    /// Whether the airspace is one to avoid or one to be careful in.
    pub policy: Policy,
    /// Where the pilot is in relation to its outline: inside or not, and
    /// the distance and direction to the nearest edge.
    pub horizontal: Horizontal,
    /// Where the pilot is in relation to its floor and ceiling.
    pub vertical: VerticalStatus,
    /// The predicted entry, when one lies within reach: up to a quarter
    /// beyond the look-ahead time, so a "near" warning can already carry
    /// one. `None` when inside, and for the few seconds a warning is held
    /// at [`Level::Entering`] after the prediction has gone.
    pub entry: Option<Entry>,
    /// When the warning reached its present level, in sample time.
    pub since_s: f64,
    /// Set on the answer in which the warning appeared, became more urgent,
    /// or came out of a silence that ran out: the moment to sound.
    pub is_new: bool,
    /// Why the warning is quiet, if the pilot silenced it.
    pub silenced: Option<Silenced>,
    /// When the silence ends, in sample time.
    pub silenced_until_s: Option<f64>,
}

impl Warning<'_> {
    /// The identity of the warning: the airspace's identifier.
    pub fn id(&self) -> &AirspaceId {
        &self.airspace.id
    }

    /// Whether the pilot may in fact be clear of the airspace's height band:
    /// the ground elevation or the pressure needed to place a limit is
    /// missing, and the limit was taken toward the pilot.
    pub fn uncertain(&self) -> bool {
        self.vertical.uncertain
    }
}

/// The state of one warning between samples.
#[derive(Debug, Clone, Copy)]
struct Track {
    level: Level,
    since_s: f64,
    /// The last time the conditions for `level` held without the extra
    /// margin: what the hold time counts from.
    confirmed_s: f64,
    was_silenced: bool,
}

/// What the pilot asked to be spared.
#[derive(Debug, Clone, Copy, Default)]
struct Quiet {
    acknowledged: Option<(Level, f64)>,
    ignored_until: Option<f64>,
}

/// Produces the cockpit's airspace warnings from a stream of samples.
#[derive(Debug, Clone)]
pub struct WarningEngine {
    config: Config,
    tracks: BTreeMap<AirspaceId, Track>,
    quiet: BTreeMap<AirspaceId, Quiet>,
    /// Recent positions, for the direction of travel.
    path: VecDeque<(f64, GeoPoint)>,
    now_s: f64,
}

impl WarningEngine {
    /// An engine for a new flight.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            tracks: BTreeMap::new(),
            quiet: BTreeMap::new(),
            path: VecDeque::new(),
            now_s: f64::NEG_INFINITY,
        }
    }

    /// The settings the engine runs with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Takes a sample and returns the warnings that hold at its time, the
    /// most urgent first. Silenced warnings are included, marked, and come last.
    ///
    /// The index may be a different one from call to call, for example when
    /// newer data has arrived: warnings are carried over by identifier.
    pub fn update<'a>(
        &mut self,
        index: &'a AirspaceIndex,
        sample: &Sample,
    ) -> Result<Vec<Warning<'a>>, SampleError> {
        if !sample.time_s.is_finite() {
            return Err(SampleError::Time);
        }
        if !sample.altitude_m.is_finite() {
            return Err(SampleError::Altitude);
        }
        let now = sample.time_s;
        if now < self.now_s {
            // Time ran backwards: a replay was rewound. What was built up
            // from later samples no longer describes the flight.
            self.tracks.clear();
            self.path.clear();
        }
        self.now_s = now;
        self.quiet.retain(|_, quiet| {
            quiet.acknowledged = quiet.acknowledged.filter(|&(_, until)| until > now);
            quiet.ignored_until = quiet.ignored_until.filter(|&until| until > now);
            quiet.acknowledged.is_some() || quiet.ignored_until.is_some()
        });

        let velocity = self.velocity(sample);
        let altitudes = sample.altitudes();
        let config = &self.config;
        let reach_s = config.look_ahead_s.max(0.0) * 1.25;
        let speed = velocity.map_or(0.0, |(speed, _)| speed);
        let search_m = (config.near.horizontal_m + config.hysteresis.horizontal_m)
            .max(speed * reach_s)
            .max(0.0);

        let mut warnings = Vec::new();
        let mut live = BTreeMap::new();
        for airspace in index.near(sample.position, search_m) {
            let policy = config.policy.policy_for(airspace);
            if policy == Policy::Free {
                continue;
            }
            let Some(horizontal) = horizontal(airspace, sample.position) else {
                continue;
            };
            let vertical = vertical_status(airspace, &altitudes);
            let entry = predict(airspace, sample, velocity, &vertical, reach_s);

            let entering_now = entry.is_some_and(|entry| entry.seconds <= config.look_ahead_s);
            let no_margin = Margin {
                horizontal_m: 0.0,
                vertical_m: 0.0,
            };
            let fresh = classify(
                config,
                policy,
                &horizontal,
                &vertical,
                entering_now,
                no_margin,
            );
            let sticky = classify(
                config,
                policy,
                &horizontal,
                &vertical,
                entry.is_some(),
                config.hysteresis,
            );

            // A warning rises at once. It falls only as far as the widened
            // margins allow, and only after the hold time.
            let previous = self.tracks.get(&airspace.id).copied();
            let held = previous.map(|track| track.level).min(sticky);
            let mut level = fresh.max(held);
            if let Some(track) = previous
                && level < Some(track.level)
                && now - track.confirmed_s < config.hold_s
            {
                level = Some(track.level);
            }
            let Some(level) = level else {
                continue;
            };

            let quiet = self.quiet.get(&airspace.id).copied().unwrap_or_default();
            let (silenced, silenced_until_s) = match (quiet.ignored_until, quiet.acknowledged) {
                (Some(until), _) => (Some(Silenced::Ignored), Some(until)),
                (None, Some((acknowledged, until))) if level <= acknowledged => {
                    (Some(Silenced::Acknowledged), Some(until))
                }
                _ => (None, None),
            };

            let escalated = previous.is_none_or(|track| level > track.level);
            let unsilenced = previous.is_some_and(|track| track.was_silenced) && silenced.is_none();
            let since_s = match previous {
                Some(track) if track.level == level => track.since_s,
                _ => now,
            };
            let confirmed_s = match previous {
                Some(track) if track.level == level && fresh < Some(level) => track.confirmed_s,
                _ => now,
            };
            live.insert(
                airspace.id.clone(),
                Track {
                    level,
                    since_s,
                    confirmed_s,
                    was_silenced: silenced.is_some(),
                },
            );
            warnings.push(Warning {
                airspace,
                level,
                policy,
                horizontal,
                vertical,
                entry: entry.filter(|_| level != Level::Inside),
                since_s,
                is_new: escalated || unsilenced,
                silenced,
                silenced_until_s,
            });
        }
        self.tracks = live;

        warnings.sort_by(|a, b| {
            let seconds =
                |warning: &Warning<'_>| warning.entry.map_or(f64::INFINITY, |e| e.seconds);
            a.silenced
                .is_some()
                .cmp(&b.silenced.is_some())
                .then(b.level.cmp(&a.level))
                .then(b.policy.cmp(&a.policy))
                .then(seconds(a).total_cmp(&seconds(b)))
                .then(
                    a.horizontal
                        .separation_m()
                        .total_cmp(&b.horizontal.separation_m()),
                )
                .then_with(|| a.airspace.id.cmp(&b.airspace.id))
        });
        Ok(warnings)
    }

    /// The pilot has seen the warning for this airspace and wants it quiet.
    ///
    /// The warning stays in the answers, marked [`Silenced::Acknowledged`],
    /// for as long as asked. It speaks again before that if it becomes more
    /// urgent than it is now: acknowledging "near" does not silence
    /// "inside". Does nothing if there is no warning for the airspace.
    pub fn acknowledge(&mut self, airspace: &AirspaceId, silence: Silence) {
        let Some(level) = self.tracks.get(airspace).map(|track| track.level) else {
            return;
        };
        let until = self.until(silence);
        self.quiet.entry(airspace.clone()).or_default().acknowledged = Some((level, until));
    }

    /// The pilot says this airspace does not apply for now: an area known
    /// to be inactive today, or one the pilot has permission for.
    ///
    /// Any warning for it stays in the answers, marked [`Silenced::Ignored`],
    /// at whatever level, until the time is up. This is the pilot's decision
    /// and responsibility; the engine cannot check it.
    pub fn ignore(&mut self, airspace: &AirspaceId, silence: Silence) {
        let until = self.until(silence);
        self.quiet
            .entry(airspace.clone())
            .or_default()
            .ignored_until = Some(until);
    }

    /// Takes back what [`WarningEngine::acknowledge`] and
    /// [`WarningEngine::ignore`] did for an airspace.
    pub fn unsilence(&mut self, airspace: &AirspaceId) {
        self.quiet.remove(airspace);
    }

    /// The time a silence ends, counted from the latest sample.
    fn until(&self, silence: Silence) -> f64 {
        match silence {
            Silence::For { seconds } => self.now_s + seconds,
            Silence::Until { time_s } => time_s,
            Silence::RestOfDay { utc_offset_s } => {
                let offset = f64::from(utc_offset_s);
                let local_day = ((self.now_s + offset) / 86_400.0).floor();
                (local_day + 1.0) * 86_400.0 - offset
            }
        }
    }

    /// Speed in metres per second and direction in degrees: over the recent
    /// path where there is enough of it, else as the sample gives them.
    fn velocity(&mut self, sample: &Sample) -> Option<(f64, f64)> {
        let now = sample.time_s;
        let window = self.config.velocity_window_s.max(0.0);
        self.path.push_back((now, sample.position));
        while self
            .path
            .front()
            .is_some_and(|&(time, _)| time < now - window)
        {
            self.path.pop_front();
        }
        if let Some(&(then, start)) = self.path.front()
            && now - then >= window / 2.0
            && now > then
        {
            let speed = start.distance_m(sample.position) / (now - then);
            return Some((speed, bearing_deg(start, sample.position)));
        }
        match (sample.ground_speed_ms, sample.course_deg) {
            (Some(speed), Some(course)) if speed.is_finite() && course.is_finite() => {
                Some((speed.max(0.0), course))
            }
            _ => None,
        }
    }
}

/// The level the pilot's position alone gives, with margins widened by `extra`.
fn classify(
    config: &Config,
    policy: Policy,
    horizontal: &Horizontal,
    vertical: &VerticalStatus,
    entering: bool,
    extra: Margin,
) -> Option<Level> {
    let within = |margin: Margin| {
        horizontal.separation_m() <= margin.horizontal_m + extra.horizontal_m
            && vertical.separation_m <= margin.vertical_m + extra.vertical_m
    };
    let nothing = Margin {
        horizontal_m: 0.0,
        vertical_m: 0.0,
    };
    if within(nothing) {
        Some(Level::Inside)
    } else if entering {
        Some(Level::Entering)
    } else if policy == Policy::Avoid && within(config.very_near) {
        Some(Level::VeryNear)
    } else if policy == Policy::Avoid && within(config.near) {
        Some(Level::Near)
    } else {
        None
    }
}

/// When and how the pilot enters an airspace within `reach_s` seconds on a
/// straight path at the present velocity and climb.
///
/// The floor and ceiling are taken where the pilot is now. A limit above
/// ground is not followed along the terrain ahead.
fn predict(
    airspace: &Airspace,
    sample: &Sample,
    velocity: Option<(f64, f64)>,
    vertical: &VerticalStatus,
    reach_s: f64,
) -> Option<Entry> {
    if reach_s <= 0.0 {
        return None;
    }
    let ahead = velocity
        .filter(|&(speed, _)| speed > 0.1)
        .and_then(|(speed, course)| destination(sample.position, course, speed * reach_s))
        .unwrap_or(sample.position);
    let over_ground = inside_stretches(&airspace.geometry, sample.position, ahead);

    // When the height is between floor and ceiling, as fractions of the reach.
    let climb = sample
        .climb_ms
        .filter(|climb| climb.is_finite())
        .unwrap_or(0.0)
        * reach_s;
    let (floor, ceiling) = (vertical.floor.low_m, vertical.ceiling.high_m);
    let in_height = if climb == 0.0 {
        (floor..=ceiling)
            .contains(&sample.altitude_m)
            .then_some((0.0, 1.0))
    } else {
        let (a, b) = (
            (floor - sample.altitude_m) / climb,
            (ceiling - sample.altitude_m) / climb,
        );
        let (from, to) = (a.min(b).max(0.0), a.max(b).min(1.0));
        (from <= to).then_some((from, to))
    };
    let (height_from, height_to) = in_height?;

    over_ground
        .into_iter()
        .filter_map(|(ground_from, ground_to)| {
            let from = ground_from.max(height_from);
            (from <= ground_to.min(height_to)).then(|| {
                let via = if ground_from >= height_from {
                    EntryVia::Side
                } else if climb > 0.0 {
                    EntryVia::Floor
                } else {
                    EntryVia::Ceiling
                };
                Entry {
                    seconds: from * reach_s,
                    via,
                }
            })
        })
        .min_by(|a, b| a.seconds.total_cmp(&b.seconds))
}
