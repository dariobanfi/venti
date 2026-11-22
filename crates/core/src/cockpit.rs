//! The cockpit: one flying session from switching it on to the saved flight.
//!
//! The shell owns the sensors. It hands every position, pressure reading
//! and byte from an external instrument to a [`FlightSession`], each with
//! the time it was taken, and a few times a second asks for a
//! [`CockpitView`] to draw and to drive the vario tone. Nothing per frame
//! or per audio sample crosses the boundary.
//!
//! Recording is built so that a crash or a kill by the system never costs
//! the track: every fix is appended to a file in the data folder the moment
//! it arrives, and that file is a valid flight file at every moment. When
//! the pilot stops, the shell imports it into the logbook and then clears
//! it. A file still there at the next launch is a recording that was cut
//! short; [`pending_recordings`] finds it so it can be imported.
//!
//! A flight records itself. [`FlightSession::record_from_takeoff`] makes the
//! session wait on the ground: it keeps the last half minute of positions
//! and writes nothing. When the flight computer notices the take-off the
//! file begins, with that half minute first. Two minutes after the glider
//! has come to rest the view says the session is over, and the shell saves
//! the flight as if the pilot had ended it. A session that has waited two
//! hours without a take-off says the same, with nothing to save. A hop of
//! less than half a minute, from take-off to coming to rest, is not a
//! flight: what was written of it is thrown away and the session waits
//! again, so that ground handling and a receiver's start-up leave nothing
//! in the logbook. The times are our own; to verify in the air.
//!
//! [`Replay`] feeds a recorded flight through a session at any speed. It is
//! how the cockpit is tried on the ground and how its screens are tested.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::DateTime;
use venti_domain::GeoPoint;
use venti_flight::fai;
use venti_flight::geodesy::Earth;
use venti_flight::igc::{self, Fix, IgcFile};
use venti_flight::live::landing::{self, NearestLanding};
use venti_flight::live::navigation::{self, Glider, Performance, Target};
use venti_flight::live::polar::{GliderClass, Polar};
use venti_flight::live::{
    self, AltitudeSource, FlightComputer, Phase, PositionSample, QnhSource, VarioSource,
    WindSource, atmosphere, tone,
};
use venti_flight::scoring;
use venti_flight::sun;
use venti_sensors::{Battery, SentenceReader};

use crate::ahead::{self, AirspaceAhead};
use crate::airspace::{AirspaceWatch, compass};
use crate::cables::{CableKind, CableSet};
use crate::cockpit_climbs::{ClimbsHeard, ClimbsNear};
use crate::cockpit_others::{OthersHeard, OthersNear};
use crate::live::{self as sharing, LiveAudience, LiveShare, LiveShareView};
use crate::route_plan::{ClosingCircleView, closing_circle_view};
use crate::task::{TaskEventView, TaskFlown, TaskReadings, TaskStatusView};
use crate::{
    AirspaceCheck, AirspaceLevel, AirspaceProximity, AirspaceSet, AirspaceWarning, Coordinate,
    CoreError, LiveOthers, RouteKind, ServerRequest, Terrain,
};

/// The cockpit's warning of a cable ahead.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CableWarning {
    /// The cable's identifier, `source:key`, to silence it by.
    pub id: String,
    /// What it is.
    pub kind: CableKind,
    /// In words: "Cable ahead: power line in 400 m, 60 m below the line
    /// you fly".
    pub words: String,
    /// How far along the course the glider comes within 150 m of it.
    pub distance_m: f64,
    /// How far the glider is above the cable's height, negative below it;
    /// `None` when the height above the ground is not known.
    pub above_m: Option<f64>,
    /// Whether the cable's height is mapped, not assumed.
    pub height_mapped: bool,
    /// Whether the pilot has silenced it.
    pub silenced: bool,
    /// How far above the cable's height the glider arrives at it on its
    /// glide, metres, negative below: what the rule goes by since
    /// 2026-11-22. A glider well above a cable now and sinking toward it
    /// is warned by this. `None` when the height above the ground is not
    /// known.
    #[uniffi(default = None)]
    pub arrival_above_m: Option<f64>,
}

/// The cables a session watches for.
#[derive(Debug, Default)]
struct CableWatch {
    set: Option<Arc<CableSet>>,
    warning: Option<CableWarning>,
    /// The cable last sounded for: one cable sounds once while it stays
    /// ahead.
    alerted: Option<String>,
    /// Cables the pilot silenced, with until when, seconds since 1970.
    silenced: Vec<(String, f64)>,
    now_unix_s: f64,
}

impl CableWatch {
    #[allow(clippy::too_many_arguments)]
    fn feed(
        &mut self,
        now_unix_s: f64,
        position: GeoPoint,
        height_above_ground_m: Option<f64>,
        track_deg: Option<f64>,
        ground_speed_m_s: Option<f64>,
        glide_over_ground: Option<f64>,
        turn_rate_deg_s: Option<f64>,
    ) {
        self.now_unix_s = now_unix_s;
        self.silenced.retain(|(_, until)| *until > now_unix_s);
        let Some(set) = self.set.as_ref() else {
            self.warning = None;
            return;
        };
        let glider = venti_flight::cables::Glider {
            position,
            height_above_ground_m,
            track_deg: track_deg.unwrap_or(0.0),
            // Without a course the glider is taken to stand: only what is
            // around it counts.
            ground_speed_m_s: if track_deg.is_some() {
                ground_speed_m_s.unwrap_or(0.0)
            } else {
                0.0
            },
            glide_over_ground,
            turn_rate_deg_s: turn_rate_deg_s.unwrap_or(0.0),
        };
        self.warning = set.ahead(&glider).map(|(found, id, words)| CableWarning {
            arrival_above_m: found.arrival_above_m,
            silenced: self.silenced.iter().any(|(silenced, _)| *silenced == id),
            kind: set.kind_of(found.index),
            id,
            words,
            distance_m: found.distance_m,
            above_m: found.above_m,
            height_mapped: found.height_mapped,
        });
        if self.warning.is_none() {
            self.alerted = None;
        }
    }
}

/// How long an external instrument may be silent, in seconds, before it
/// counts as disconnected and the phone's own pressure sensor takes over.
const INSTRUMENT_SILENCE_S: f64 = 3.0;

/// A flight that records itself begins this many seconds before its
/// take-off was noticed: the run, and a little of the standing before it.
const LEAD_S: f64 = 30.0;

/// A flight that records itself is over this many seconds after the glider
/// came to rest: long enough to be sure of the landing, and over before
/// the pilot has packed and driven away.
const SAVE_AFTER_LANDING_S: f64 = 120.0;

/// A flight that records itself and is shorter than this, in seconds from
/// take-off to coming to rest, is no flight: a hop while ground handling,
/// or a take-off the sensors made up.
const SHORTEST_FLIGHT_S: f64 = 30.0;

/// A session that waits for a take-off gives up after this many seconds:
/// waiting keeps the phone's receivers on.
const WAIT_FOR_TAKEOFF_S: f64 = 2.0 * 3600.0;

/// Fixes written between forced writes to the disk. The system keeps
/// written data safe across an app crash by itself; forcing matters only if
/// the phone loses power, where this bounds the loss to half a minute.
const FIXES_PER_SYNC: u32 = 30;

/// One position from the phone's location service.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct LocationSample {
    /// When the position was taken, seconds since 1970-01-01 UTC.
    pub time_unix_s: f64,
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
    /// Altitude above sea level in metres, if known.
    pub altitude_m: Option<f64>,
    /// Speed over the ground in metres per second, if the receiver
    /// measured it.
    pub ground_speed_ms: Option<f64>,
    /// Direction of travel in degrees, if the receiver measured it.
    pub course_deg: Option<f64>,
}

/// Where the flight is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FlightPhase {
    /// Before take-off.
    Ground,
    /// In the air.
    Flying,
    /// Back on the ground after a flight.
    Landed,
}

/// Why a session that records its flight by itself is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SessionOver {
    /// The glider has landed and stayed where it is: the flight is to be
    /// saved.
    Landed,
    /// Nobody took off while the session waited: there is nothing to save.
    NoTakeoff,
}

/// Where the climb rate comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VarioSourceKind {
    /// An external instrument's own vario.
    Instrument,
    /// A pressure sensor: the phone's, or an instrument's.
    Pressure,
    /// Satellite altitude: slow, the last resort.
    Satellite,
}

/// A wind.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindView {
    /// The direction it comes from, degrees clockwise from north.
    pub from_deg: f64,
    /// Its speed in metres per second.
    pub speed_ms: f64,
}

/// The wind as last measured in one band of height: the wind in the
/// valley and the wind above the ridges are two winds.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct WindAtHeightView {
    /// The band's floor, metres above sea level.
    pub floor_m: f64,
    /// The band's ceiling, metres above sea level.
    pub ceiling_m: f64,
    /// The direction the wind came from, degrees clockwise from north.
    pub from_deg: f64,
    /// Its speed in metres per second.
    pub speed_ms: f64,
    /// How long ago it was measured, seconds.
    pub age_s: f64,
    /// Whether the glider is in this band now.
    pub is_here: bool,
    /// How fast the glider climbed while it circled through the band,
    /// metres per second. Absent for a band it circled in only briefly.
    pub climb_ms: Option<f64>,
}

/// The thermal the glider is circling in, or the last one it left.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ThermalView {
    /// Height gained in it, metres.
    pub gain_m: f64,
    /// Average climb in it, metres per second.
    pub climb_ms: f64,
}

/// A thermal the glider has left, for the map: where it circled and what
/// it gave.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ThermalMarkView {
    /// The middle of the circles flown in it.
    pub position: Coordinate,
    /// Average climb in it, metres per second.
    pub climb_ms: f64,
    /// Height gained in it, metres.
    pub gain_m: f64,
    /// The altitude it was left at, metres.
    pub top_altitude_m: f64,
    /// Its climb as a map writes it: "+2.4".
    pub label: String,
}

/// One place the glider flew through in the thermal it is circling in.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct LiftPointView {
    /// Latitude in degrees of where the air it flew through there is now.
    pub lat: f64,
    /// Longitude in degrees, likewise.
    pub lon: f64,
    /// The climb rate around there, metres per second.
    pub climb_ms: f64,
    /// That climb beside the mean of the picture, from 0 to 1: one half is
    /// the mean, 1 is a metre per second or more above it, 0 as much
    /// below. For the size of a dot: its colour says how good the climb
    /// is, its size where in this thermal it is better.
    pub strength: f64,
}

/// Where the lift is in the thermal the glider is circling in.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LiftCentreView {
    /// Where to centre the next circle: the middle of the circles flown,
    /// moved toward the side where the climb was stronger.
    pub position: Coordinate,
    /// The circle to fly around it, with the radius of the circles flown:
    /// a closed line for the map.
    pub outline: Vec<Coordinate>,
    /// The radius of the circles flown, metres.
    pub radius_m: f64,
    /// How far `position` lies from the middle of the circles flown,
    /// metres. Never more than the radius.
    pub shift_m: f64,
    /// The direction it lies in from there.
    pub toward_deg: f64,
    /// How much stronger the climb is on that side than on average,
    /// metres per second.
    pub stronger_by_ms: f64,
    /// Whether that is so little that the circle counts as centred: less
    /// than a fifth of its radius.
    pub is_centred: bool,
    /// The same in words: "Centred", or "Stronger 20 m NW".
    pub words: String,
}

/// The thermal the glider is circling in, as its last circles show it. The
/// air of a thermal moves with the wind, so every place is shown where its
/// air is now: the circles stay circles and the lift stays where it was
/// found.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LiftView {
    /// The places flown through in the last fifty seconds, oldest first.
    pub points: Vec<LiftPointView>,
    /// Where the lift is. Absent until most of a circle is flown, and
    /// while what was flown is no circle.
    pub centre: Option<LiftCentreView>,
    /// Whether a wind was known to move the places with. Without one they
    /// are where the glider was over the ground.
    pub drifted: bool,
}

/// One point of the track behind the glider.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TrailPointView {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
    /// Climb rate there, metres per second.
    pub climb_ms: f64,
}

/// What the vario should sound like right now. The shell synthesises it.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ToneView {
    /// Pitch in hertz.
    pub frequency_hz: f64,
    /// Length of one beep-and-pause cycle, seconds.
    pub cycle_s: f64,
    /// The share of each cycle that sounds. One is a continuous tone.
    pub duty: f64,
}

/// The state of the external instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct InstrumentView {
    /// Whether it has sent something understandable in the last seconds.
    pub talking: bool,
    /// Its battery charge in percent, if it reports one.
    pub battery_percent: Option<u8>,
    /// Sentences understood since the session began.
    pub sentences: u64,
    /// Lines thrown away as damaged since the session began.
    pub damaged: u64,
}

/// Everything the cockpit screen shows. A value is absent until the sensors
/// have supplied what it needs. Speeds are in metres per second and angles
/// in degrees clockwise from north.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CockpitView {
    /// Where the flight is in its life.
    pub phase: FlightPhase,
    /// Whether fixes are being written to a recording.
    pub recording: bool,
    /// Whether the session waits for the take-off to begin its recording
    /// by itself. Nothing is written while it waits.
    pub waiting_for_takeoff: bool,
    /// Why the session is over by itself, once it is: the shell then ends
    /// it as if the pilot had, and saves what was recorded. Only for a
    /// session that records its flight by itself.
    pub over: Option<SessionOver>,
    /// Seconds until a flight that has landed is saved by itself. Absent
    /// before the landing, and for a session the pilot ends.
    pub saves_in_s: Option<i64>,
    /// Fixes written to the recording so far.
    pub recorded_fixes: u32,
    /// Seconds in the air: since take-off, or from take-off to landing.
    pub flight_time_s: Option<i64>,
    /// Seconds since the last position arrived. Large means the satellite
    /// fix is lost.
    pub position_age_s: Option<f64>,
    /// The latest position.
    pub position: Option<Coordinate>,
    /// The best altitude above sea level, metres.
    pub altitude_m: Option<f64>,
    /// Height above the take-off, metres.
    pub height_above_takeoff_m: Option<f64>,
    /// The height of the ground under the glider, metres above mean sea
    /// level, where the app holds terrain for the place.
    pub ground_elevation_m: Option<f64>,
    /// Height above the ground under the glider, metres: the altitude
    /// less the terrain's height there. The terrain is a model of the
    /// bare ground a pixel of about 13 m wide: it knows no trees, cables
    /// or buildings, and on a steep slope the ground a few metres to the
    /// side is far higher or lower.
    pub height_above_ground_m: Option<f64>,
    /// The highest altitude of the flight, metres.
    pub max_altitude_m: Option<f64>,
    /// The climb rate now.
    pub climb_ms: Option<f64>,
    /// Where the climb rate comes from.
    pub vario_source: Option<VarioSourceKind>,
    /// The climb rate averaged over twenty seconds.
    pub average_climb_ms: Option<f64>,
    /// Speed over the ground.
    pub ground_speed_ms: Option<f64>,
    /// Direction of travel.
    pub course_deg: Option<f64>,
    /// The direction of travel as a reading, in degrees and as one of
    /// eight compass points: "285° W". Absent while the glider moves
    /// slower than 2 m/s over the ground, where a course says nothing,
    /// and whenever there is no course.
    pub course_words: Option<String>,
    /// Metres forward per metre of height lost. Absent when not losing
    /// height.
    pub glide_ratio: Option<f64>,
    /// The wind, from the last time the glider circled.
    pub wind: Option<WindView>,
    /// How long ago that was, seconds. A wind from half an hour ago and
    /// another height is a memory, not a measurement: the shell says its
    /// age once it is more than a few minutes.
    pub wind_age_s: Option<f64>,
    /// Said while the wind last measured, if it is not older than twenty
    /// minutes, leaves the glider less than 10 km/h of way into it at its
    /// trim speed: "Into the wind 4 km/h", or that the wind is as strong
    /// as the glider is fast. Absent otherwise.
    pub strong_wind: Option<String>,
    /// The wind in each band of 200 m the glider has circled in, the
    /// highest first, each as last measured there.
    pub winds_by_height: Vec<WindAtHeightView>,
    /// Whether the glider is circling in lift. The shell shows its thermal
    /// page while this is true.
    pub circling: bool,
    /// The thermal it is in, or the last one it left.
    pub thermal: Option<ThermalView>,
    /// While circling: the last circles and where the lift is among them.
    pub lift: Option<LiftView>,
    /// The thermals it has left that gave fifty metres or more, oldest
    /// first, the last twelve: marks for the map.
    pub thermals: Vec<ThermalMarkView>,
    /// Where the glider took off.
    pub takeoff: Option<Coordinate>,
    /// Distance back to the take-off, metres.
    pub distance_to_takeoff_m: Option<f64>,
    /// The direction to fly to reach the take-off.
    pub bearing_to_takeoff_deg: Option<f64>,
    /// The track of the last three minutes, oldest first.
    pub trail: Vec<TrailPointView>,
    /// The vario tone for this moment, or none for silence. Silent on the
    /// ground.
    pub tone: Option<ToneView>,
    /// The external instrument, if one has sent anything.
    pub instrument: Option<InstrumentView>,
    /// The airspace that matters next, as a reading that is always there:
    /// which one, how far its outline is and in which direction, and how
    /// it lies in height. It is of the same position as
    /// [`FlightSession::airspace_warnings`] and stays while a warning is
    /// up. Absent when no airspace to avoid or to be careful in has its
    /// outline within five kilometres over the ground, and that is "none
    /// near" only while [`FlightSession::airspace_check`] says `Checking`:
    /// it is absent as well without airspace, before the first position
    /// with a height and where the pilot's airspace does not reach, and
    /// once positions have stopped it is the last one made.
    pub airspace_proximity: Option<AirspaceProximity>,
    /// The competition page's readings for the task being flown: the
    /// next turnpoint and goal with the altitude needed in the wind, the
    /// start gate, the speed section's average. Absent without a task and
    /// until there is a position.
    #[uniffi(default = None)]
    pub task_readings: Option<TaskReadings>,
    /// Where the wind comes from: the last circles, or a turn on a glide
    /// since. Absent without a wind.
    #[uniffi(default = None)]
    pub wind_source: Option<WindSourceKind>,
    /// The wind in a line, with where it comes from and how old it is:
    /// "11 km/h from W, from your last glide, 2 min old", "14 km/h from
    /// NE, from your last circles, under a minute old". Absent without a
    /// wind.
    #[uniffi(default = None)]
    pub wind_words: Option<String>,
    /// The glider's speed through the air, estimated: its velocity over
    /// the ground less the wind. Absent on the ground, without a wind,
    /// with a wind older than twenty minutes, and where the result is no
    /// speed a glider flies. An estimate, as good as the wind is.
    #[uniffi(default = None)]
    pub airspeed_ms: Option<f64>,
    /// The same as a reading that says it is an estimate: "about 36
    /// km/h".
    #[uniffi(default = None)]
    pub airspeed_words: Option<String>,
    /// The direction the glider points in, estimated the same way, and
    /// absent with the airspeed.
    #[uniffi(default = None)]
    pub heading_deg: Option<f64>,
    /// The same as a reading: "about 350° N".
    #[uniffi(default = None)]
    pub heading_words: Option<String>,
    /// Said while the wind carries the glider backwards on a glide: it
    /// moves over the ground against the direction it points in, by that
    /// estimate. "The wind carries you back: 4 km/h backwards". Absent
    /// otherwise, and while circling, where every circle has a half that
    /// is flown backwards in such a wind.
    #[uniffi(default = None)]
    pub carried_back: Option<String>,
    /// The nearest landing within glide, among the landings handed over
    /// with [`FlightSession::use_landings`], or the caution that none is.
    /// Absent on the ground, without landings and without a height.
    #[uniffi(default = None)]
    pub nearest_landing: Option<LandingReading>,
    /// How fast the air itself rises or sinks, metres per second: the
    /// climb with the polar's sink at the estimated airspeed given back.
    /// Only for a pilot who has set a glider class or a polar
    /// ([`FlightSession::set_glider_class`], [`FlightSession::set_polar`]),
    /// only on a glide that is not turning, and absent with the airspeed.
    #[uniffi(default = None)]
    pub netto_ms: Option<f64>,
    /// The same as a reading: "air −1.3 m/s", "air +0.4 m/s".
    #[uniffi(default = None)]
    pub netto_words: Option<String>,
    /// The speed to fly on this glide, as a hint in words. Absent as the
    /// netto is.
    #[uniffi(default = None)]
    pub speed_to_fly: Option<SpeedToFlyView>,
    /// Where the altitude comes from, said plainly: "Altitude from the
    /// pressure sensor, levelled to GPS", "Altitude from GPS". Absent
    /// without an altitude.
    #[uniffi(default = None)]
    pub altitude_source_words: Option<String>,
    /// The QNH, hectopascals: taken on the ground before take-off from
    /// the pressure and the height of the ground there, or set by hand
    /// ([`FlightSession::set_qnh`]). Absent without a pressure sensor,
    /// and until the session has stood on ground whose height it holds.
    #[uniffi(default = None)]
    pub qnh_hpa: Option<f64>,
    /// What an altimeter set to that QNH shows now, metres: the altitude
    /// airspace limits above sea level are written for, and the one the
    /// airspace warnings measure them against. It differs from
    /// `altitude_m` by what the day's temperature does to an altimeter;
    /// limits above ground stay on `altitude_m`. Absent without a fresh
    /// pressure: the warnings then measure every limit but flight levels
    /// against `altitude_m`.
    #[uniffi(default = None)]
    pub qnh_altitude_m: Option<f64>,
    /// The QNH as a reading, with what the airspace warnings go by: "QNH
    /// 1018 hPa, from the ground at take-off · airspace above sea level
    /// measured at it", "QNH 1021 hPa, set by hand · no pressure now:
    /// airspace above sea level by GPS".
    #[uniffi(default = None)]
    pub qnh_words: Option<String>,
    /// Whether the flight is never taken for landed
    /// ([`FlightSession::set_never_land`]): the pilot ends it.
    #[uniffi(default = false)]
    pub never_lands: bool,
}

/// Where a wind was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum WindSourceKind {
    /// Around nearly a whole circle.
    Circling,
    /// In a turn of a third of a circle or more on a glide: less sure.
    Glide,
}

/// The class a paraglider is certified in, for a polar to start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GliderClassKind {
    /// EN A.
    A,
    /// EN B.
    B,
    /// EN C.
    C,
    /// EN D.
    D,
    /// CCC.
    Ccc,
}

/// The speed to fly on a glide, by the polar the pilot has set, the
/// estimated head wind and the air's rise or fall. A hint: the airspeed
/// it is worked out from is itself an estimate, so the cockpit says what
/// to do and why, not a number to hold.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SpeedToFlyView {
    /// "Speed up: sinking air and head wind", "Speed up: head wind",
    /// "Trim: you are in rising air", "Trim: tail wind", "Trim".
    pub words: String,
    /// How much speed bar: 0 at trim, 1 at the polar's top speed.
    pub bar: f64,
    /// Whether the hint is to speed up: a quarter of the bar or more.
    pub speed_up: bool,
}

/// The nearest landing within glide, as a standing reading.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LandingReading {
    /// Whether a landing is within glide. When false this is a caution,
    /// and the place named, if any, is the one that misses by least.
    pub within_glide: bool,
    /// Which of the landings handed over it is, counted from zero.
    pub index: Option<u32>,
    /// Its name.
    pub name: Option<String>,
    /// How far it is, metres.
    pub distance_m: Option<f64>,
    /// The direction to it, degrees clockwise from north.
    pub bearing_deg: Option<f64>,
    /// How far above it the glide is expected to end, metres; negative
    /// when it comes down before it.
    pub arrival_height_m: Option<f64>,
    /// Where the straight glide to it meets the ground first, metres
    /// from the glider.
    pub ground_in_the_way_m: Option<f64>,
    /// "Landing Musterwiese: 2.1 km, +340 m"; "No landing within glide:
    /// Musterwiese 6.2 km, 180 m short"; "No landing within glide:
    /// Musterwiese 3.0 km, a ridge in the way"; "No landing within
    /// glide".
    pub words: String,
}

/// How the flight scores as a cross-country flight so far, by the same
/// rules as a finished flight in the logbook.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LiveScore {
    /// What it scores as.
    pub kind: RouteKind,
    /// For a triangle, whether it is closed to within 5%.
    pub closed: bool,
    /// The scoring distance so far, metres.
    pub distance_m: f64,
    /// The points so far.
    pub points: f64,
    /// For a triangle, the gap between its start and its finish, metres:
    /// how far there is to fly to close it.
    pub closing_m: Option<f64>,
    /// The route the score runs along, for drawing.
    pub route: Vec<Coordinate>,
    /// For a triangle still open, the circle around its start that its
    /// finish has to come into, with the distance to its edge from the
    /// glider and what the triangle scores closed. It is there also while
    /// a free flight scores more, so long as the flight makes a triangle
    /// that counts. Absent for a closed triangle and without one.
    #[uniffi(default = None)]
    pub closing: Option<ClosingCircleView>,
}

/// Where to fly for an FAI triangle: a triangle whose shortest side is at
/// least 28% of its perimeter, which the leagues score highest.
///
/// The areas stand on the longest straight line the flight has covered so
/// far, taken as one side of the triangle: a third corner inside either
/// of them makes it an FAI triangle. While the glider still flies away
/// from where it started, the line and the areas move with it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FaiAreas {
    /// The two ends of the side the areas stand on.
    pub side: Vec<Coordinate>,
    /// The outline of each area, closed: one on each side of the side.
    pub outlines: Vec<Vec<Coordinate>>,
    /// The perimeter of the smallest FAI triangle on that side, metres.
    pub smallest_m: f64,
    /// The perimeter of the largest.
    pub largest_m: f64,
    /// Whether a corner where the glider is now would make an FAI
    /// triangle of the side.
    pub is_inside: bool,
}

/// How much of the day is left where the glider is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct DaylightLeft {
    /// Sunset, seconds since 1970-01-01 UTC. Absent where the sun does
    /// not set that day.
    pub sunset_unix_s: Option<i64>,
    /// The end of civil twilight, from when it is night.
    pub dusk_unix_s: Option<i64>,
    /// Minutes until sunset; negative once the sun has set.
    pub minutes_to_sunset: Option<i64>,
    /// Minutes until the end of civil twilight; negative once it is night.
    pub minutes_to_dusk: Option<i64>,
}

/// Seconds between the positions kept for scoring the flight in the air.
const SCORE_FIX_EVERY_S: f64 = 10.0;

/// The score is worked out again once this many new positions are there:
/// every half minute.
const SCORE_AGAIN_AFTER_FIXES: usize = 3;

/// The most positions kept for scoring: more than a day in the air.
const SCORE_FIXES_AT_MOST: usize = 10_000;

/// The FAI triangle areas are offered once the longest straight line flown
/// is this long, in metres. Before that they are a clutter of small
/// shapes around the take-off.
const FAI_AREAS_FROM_M: f64 = 3_000.0;

/// A place for the cockpit to navigate to.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TargetPlace {
    /// Its name.
    pub name: String,
    /// Where it is.
    pub position: Coordinate,
    /// Its height above sea level, metres, if known.
    pub elevation_m: Option<f64>,
}

/// Whether the glider is expected to reach a place by gliding to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Reach {
    /// Expected to arrive at least a hundred metres above it.
    Within,
    /// Expected to arrive, with less than that to spare.
    Just,
    /// Not expected to arrive: too far for the height, or the wind is too
    /// strong to make way towards it.
    Beyond,
    /// Not known: the place has no height, or the glider has none yet.
    Unknown,
}

/// How one place lies within the glider's reach, by the glider's trim
/// speed and glide ratio and the wind last measured, in a straight line.
/// Where the session has terrain, the ground under that line is looked
/// at, and a ridge the line would meet puts the place out of reach.
/// Where it has none, a ridge in the way is not seen.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ReachView {
    /// How far it is, metres.
    pub distance_m: f64,
    /// The direction to it, degrees clockwise from north.
    pub bearing_deg: f64,
    /// How far above the place the glider is expected to arrive, metres;
    /// negative when it is expected to come down before it. Absent when
    /// that cannot be worked out.
    pub arrival_height_m: Option<f64>,
    /// Whether that is within reach.
    pub reach: Reach,
    /// Where the straight glide to the place meets the ground before it
    /// gets there, metres from the glider: a ridge in the way. The place
    /// is then `Beyond`, whatever the arrival height says. `None` when
    /// the line is clear as far as the ground is known, and without
    /// terrain.
    pub ground_in_the_way_m: Option<f64>,
}

/// The best of the landings the glider could go to, for one line in the
/// cockpit.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LandingOption {
    /// Which of the places given it is, counted from zero.
    pub index: u32,
    /// Its name.
    pub name: String,
    /// How it lies within reach. `Beyond` means that no landing given is
    /// within reach, and this is the one that misses by least.
    pub reach: ReachView,
    /// How many of the places given are within reach, the ones that are
    /// only just so included.
    pub in_reach: u32,
}

/// How far the glider glides: the edge of what a straight glide reaches
/// over the ground as it is, for a line on the map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GlideRange {
    /// The edge, as a closed ring around the glider: a place every five
    /// degrees, and the first again at the end. In each direction it lies
    /// where the glider's own glide, in the wind as last worked out, comes
    /// down to the reserve over the ground. What lies behind a ridge is
    /// outside it, although a pilot may get there another way.
    pub outline: Vec<Coordinate>,
    /// The height left over the ground at the edge, metres.
    pub reserve_m: f64,
    /// How far the edge is from the glider where it is nearest, metres.
    pub nearest_m: f64,
    /// And where it is farthest.
    pub farthest_m: f64,
    /// Whether the edge lies, somewhere, where the ground the app holds
    /// ends and not where the glide does: there the glider reaches at
    /// least that far.
    pub is_cut_short: bool,
}

/// The way to the place the cockpit navigates to.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct WayToTarget {
    /// The place's name.
    pub name: String,
    /// Whether it is the take-off of this flight, which the cockpit
    /// navigates back to when the pilot has chosen nothing else.
    pub is_takeoff: bool,
    /// Where it is.
    pub position: Coordinate,
    /// Distance to it over the ground, metres.
    pub distance_m: f64,
    /// The direction to fly, degrees clockwise from north.
    pub bearing_deg: f64,
    /// How far the glider is above the place now, metres; negative when
    /// below it. Absent without the place's height.
    pub height_above_m: Option<f64>,
    /// The glide ratio over the ground needed to arrive at the place's
    /// height. Absent when the glider is not above it.
    pub required_glide: Option<f64>,
    /// The glide ratio over the ground the glider is expected to make
    /// towards the place: its own glide at trim speed, in the wind as last
    /// measured. Absent when the wind is too strong to get there.
    pub expected_glide: Option<f64>,
    /// How far above the place the glider is expected to arrive, metres;
    /// negative when it is expected to fall short. It assumes air that
    /// neither rises nor sinks on the way: advice, not a promise.
    pub arrival_height_m: Option<f64>,
    /// How long the glide is expected to take, seconds.
    pub time_to_go_s: Option<f64>,
    /// Whether a wind was known for the prediction. Without one, still air
    /// is assumed, and the screen should say so.
    pub wind_known: bool,
    /// Where the straight glide to the place meets the ground before it
    /// gets there, metres from the glider: a ridge in the way, whatever
    /// the arrival height says. `None` when the line is clear as far as
    /// the ground is known, and without terrain.
    pub ground_in_the_way_m: Option<f64>,
}

/// A recording that was not imported into the logbook, found at launch.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PendingRecording {
    /// Its name, to clear it once it has been imported.
    pub name: String,
    /// The flight file's text.
    pub igc_text: String,
}

/// One flying session. The shell creates it when the pilot opens the
/// cockpit and keeps it until they close it.
#[derive(Debug, uniffi::Object)]
pub struct FlightSession {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    folder: PathBuf,
    computer: FlightComputer,
    reader: SentenceReader,
    instrument: Option<Instrument>,
    pressure_source: Option<PressureSource>,
    recorder: Option<Recorder>,
    /// While the recording waits for the take-off: since when, and the
    /// last positions on the ground.
    lead: Option<Lead>,
    /// Whether the flight records itself, and so also ends by itself.
    by_itself: bool,
    /// Whether the pilot said "record now": then nothing is thrown away
    /// for being short.
    by_hand: bool,
    last_position_s: Option<f64>,
    /// The place the pilot chose to fly to. Without one the cockpit
    /// navigates back to the take-off.
    target: Option<Target>,
    glider: Glider,
    /// The polar, once the pilot has set a glider class or a polar.
    /// Without one nothing of netto or speed to fly is worked out.
    polar: Option<Polar>,
    /// The landings to keep a standing reading of, and the reading of
    /// the latest position.
    landings: Vec<Target>,
    landing: Option<LandingReading>,
    /// Whether the flight is never taken for landed.
    never_land: bool,
    /// The QNH the pilot set by hand.
    qnh_by_hand: Option<f64>,
    /// The airspace warnings, once the shell has handed over the airspace.
    watch: Option<AirspaceWatch>,
    /// The cables to watch for, once the shell has handed them over, and
    /// the warning of the latest position.
    cables: CableWatch,
    /// The height of the ground, once the shell has handed over terrain.
    terrain: Option<Arc<Terrain>>,
    /// The ground under the latest position, metres above mean sea level,
    /// where the terrain covers it.
    ground_m: Option<f64>,
    /// The flight since take-off, a position every ten seconds, for
    /// scoring it in the air.
    flown: Vec<Fix>,
    /// The latest score, and how many positions it was worked out from.
    score: Option<(usize, scoring::Scores)>,
    /// The two fixes of `flown` that lie farthest apart, and how far.
    longest: Option<(GeoPoint, GeoPoint, f64)>,
    /// The task being flown, if the pilot set one.
    task: Option<TaskFlown>,
    /// The flight as others may watch it, if the pilot shares it.
    live: Option<LiveShare>,
    /// The aircraft other networks hear around the glider, with their
    /// tracks of the last minutes, from the answers the shell handed in.
    others: OthersHeard,
    /// The climbs of the last minutes around the glider, from the last
    /// answer the shell handed in.
    climbs: ClimbsHeard,
    /// Whether a recorded flight was played into this session. What is
    /// replayed is never shared: it would put a glider on other people's
    /// maps that is not there.
    replayed: bool,
}

/// A recording that waits for the take-off.
#[derive(Debug)]
struct Lead {
    /// When the session began to wait.
    since_s: f64,
    /// The positions of the last [`LEAD_S`] seconds, oldest first, as the
    /// recorder takes them.
    fixes: VecDeque<(LocationSample, GeoPoint, Option<f64>)>,
}

impl Lead {
    /// Keeps a position, and lets go of those older than the lead.
    fn keep(
        &mut self,
        sample: LocationSample,
        position: GeoPoint,
        pressure_altitude_m: Option<f64>,
    ) {
        while self
            .fixes
            .front()
            .is_some_and(|(kept, _, _)| kept.time_unix_s < sample.time_unix_s - LEAD_S)
        {
            self.fixes.pop_front();
        }
        self.fixes
            .push_back((sample, position, pressure_altitude_m));
    }
}

/// Which sensor is supplying pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PressureSource {
    Phone,
    Instrument,
    Replay,
}

impl Inner {
    /// Goes back to waiting for the take-off, as if the session had just
    /// begun: the flight computer starts anew and the recording is empty.
    fn wait_again(&mut self, now_s: f64) {
        if let Some(recorder) = self.recorder.as_mut() {
            // A file that cannot be removed is found at the next launch
            // as a recording cut short; that is the lesser harm.
            let _ = recorder.restart();
        }
        self.lead = Some(Lead {
            since_s: now_s,
            fixes: VecDeque::new(),
        });
        self.computer = FlightComputer::new(live::Config::default());
        self.computer.set_never_land(self.never_land);
        self.computer.set_qnh(self.qnh_by_hand);
        self.flown.clear();
        self.score = None;
        self.longest = None;
    }

    /// How the glider glides: by its polar where the pilot has set one,
    /// else by trim speed and glide ratio.
    fn performance(&self) -> Performance {
        match self.polar {
            Some(polar) => Performance::Polar(polar),
            None => Performance::Trim(self.glider),
        }
    }

    /// Takes an altitude on the standard atmosphere from a pressure sensor.
    ///
    /// Two pressure sensors disagree by a constant of several metres. When
    /// the supplier changes, the computer starts its pressure history
    /// again, so the step between them is not taken for a climb.
    fn pressure_altitude(&mut self, source: PressureSource, time_s: f64, altitude_m: f64) {
        if self
            .pressure_source
            .is_some_and(|current| current != source)
        {
            self.computer.restart_pressure();
        }
        self.pressure_source = Some(source);
        self.computer.pressure_altitude(time_s, altitude_m);
    }
}

#[derive(Debug, Clone, Copy)]
struct Instrument {
    last_heard_s: f64,
    last_pressure_s: Option<f64>,
    battery_percent: Option<u8>,
    sentences: u64,
    damaged: u64,
}

#[uniffi::export]
impl FlightSession {
    /// Starts a session. `data_folder` is the folder the core was created
    /// with; recordings are kept in it until they are in the logbook.
    #[uniffi::constructor]
    pub fn new(data_folder: String) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                folder: PathBuf::from(data_folder),
                computer: FlightComputer::new(live::Config::default()),
                reader: SentenceReader::new(),
                instrument: None,
                pressure_source: None,
                recorder: None,
                lead: None,
                by_itself: false,
                by_hand: false,
                last_position_s: None,
                target: None,
                glider: Glider::default(),
                polar: None,
                landings: Vec::new(),
                landing: None,
                never_land: false,
                qnh_by_hand: None,
                watch: None,
                cables: CableWatch::default(),
                terrain: None,
                ground_m: None,
                flown: Vec::new(),
                score: None,
                longest: None,
                task: None,
                live: None,
                others: OthersHeard::default(),
                climbs: ClimbsHeard::default(),
                replayed: false,
            }),
        })
    }

    /// Begins to share this flight with others while it is under way.
    /// Nothing is sent before take-off, and nothing by this call: ask
    /// [`FlightSession::live_request`] once a second for what to send.
    ///
    /// `server` is the address of the pilot's Venti server, as the account
    /// has it. `random` is 26 bytes from the system's source of random
    /// numbers for secrets (`SecRandomCopyBytes`, `SecureRandom`): the
    /// link is made of them, and whoever has the link sees the flight.
    /// Sharing again begins a new track with a new link.
    pub fn share_live(
        &self,
        server: String,
        random: Vec<u8>,
        audience: LiveAudience,
    ) -> Result<LiveShareView, CoreError> {
        let mut inner = self.lock();
        if inner.replayed {
            return Err(CoreError::Server {
                message: "A replayed flight is not shared.".to_owned(),
            });
        }
        let share = LiveShare::begin(&server, &random, audience)?;
        let view = share.view(inner.last_position_s.unwrap_or(0.0));
        inner.live = Some(share);
        Ok(view)
    }

    /// Where the sharing of this flight stands, if it is shared.
    pub fn live_share(&self, now_unix_s: f64) -> Option<LiveShareView> {
        self.lock()
            .live
            .as_ref()
            .map(|share| share.view(now_unix_s))
    }

    /// Changes who may see the flight. Fewer people is said to the server
    /// at once, more people with the next positions.
    pub fn set_live_audience(&self, audience: LiveAudience) {
        if let Some(share) = self.lock().live.as_mut() {
            share.set_audience(audience);
        }
    }

    /// The request to send to the server now for the shared flight, if one
    /// is due. Ask once a second. Send it with the pilot's token, then
    /// hand the answer to [`FlightSession::live_answer`], or say with
    /// [`FlightSession::live_no_answer`] that there was none: until one of
    /// the two is called, nothing further is handed out.
    pub fn live_request(&self, now_unix_s: f64) -> Option<ServerRequest> {
        self.lock().live.as_mut()?.request(now_unix_s)
    }

    /// Takes the server's answer to the request [`FlightSession::live_request`]
    /// handed out. `retry_after_s` is the answer's `Retry-After`.
    pub fn live_answer(
        &self,
        status: u16,
        body: Vec<u8>,
        retry_after_s: Option<u32>,
        now_unix_s: f64,
    ) {
        if let Some(share) = self.lock().live.as_mut() {
            share.answer(status, &body, retry_after_s, now_unix_s);
        }
    }

    /// Says that the request got no answer: no network, or a time-out.
    pub fn live_no_answer(&self, now_unix_s: f64) {
        if let Some(share) = self.lock().live.as_mut() {
            share.no_answer(now_unix_s);
        }
    }

    /// The pilot ends the flight: the server is told that the glider has
    /// landed, with the next request, which is due at once.
    pub fn end_live(&self) {
        if let Some(share) = self.lock().live.as_mut() {
            share.set_landed(true);
        }
    }

    /// The request that removes the shared flight from the server, for the
    /// pilot who wants it gone now and not after two days. Nothing more is
    /// sent for this flight afterwards.
    pub fn delete_live(&self) -> Option<ServerRequest> {
        self.lock().live.as_mut().map(LiveShare::deletion)
    }

    /// How much of the day is left at the glider's position: sunset and
    /// the end of civil twilight, for a level horizon at sea level. In a
    /// valley the sun is gone behind the mountains earlier. `None` until
    /// there is a position.
    pub fn daylight(&self, now_unix_s: f64) -> Option<DaylightLeft> {
        let position = self.lock().computer.state(now_unix_s).position?;
        let now = DateTime::from_timestamp(now_unix_s as i64, 0)?;
        let light = sun::daylight_at(now, position);
        let minutes_to = |event: Option<DateTime<chrono::Utc>>| {
            event.map(|event| (event - now).num_seconds().div_euclid(60))
        };
        Some(DaylightLeft {
            sunset_unix_s: light.sunset.map(|sunset| sunset.timestamp()),
            dusk_unix_s: light.civil_dusk.map(|dusk| dusk.timestamp()),
            minutes_to_sunset: minutes_to(light.sunset),
            minutes_to_dusk: minutes_to(light.civil_dusk),
        })
    }

    /// How the flight scores as a cross-country flight so far. Worked out
    /// anew every half minute of flight; asking more often costs nothing.
    /// `None` on the ground and for the first moments in the air.
    pub fn live_score(&self) -> Option<LiveScore> {
        // The scoring runs on a copy, outside the lock, so that positions
        // keep arriving while it does.
        let flown = {
            let inner = self.lock();
            match &inner.score {
                Some((scored, score)) if inner.flown.len() < scored + SCORE_AGAIN_AFTER_FIXES => {
                    return Some(live_score(score, &inner.flown));
                }
                _ => inner.flown.clone(),
            }
        };
        let score = scoring::scores(&flown)?;
        let view = live_score(&score, &flown);
        self.lock().score = Some((flown.len(), score));
        Some(view)
    }

    /// Where to fly for an FAI triangle, once the flight has covered
    /// three kilometres in a straight line. Cheap: ask with the score.
    pub fn fai_areas(&self) -> Option<FaiAreas> {
        let (from, to, here) = {
            let inner = self.lock();
            let (from, to, length) = inner.longest?;
            if length < FAI_AREAS_FROM_M {
                return None;
            }
            (from, to, inner.flown.last()?.position)
        };
        let areas = fai::areas(from, to)?;
        let coordinate = |point: &GeoPoint| Coordinate {
            lat: point.lat(),
            lon: point.lon(),
        };
        Some(FaiAreas {
            side: vec![coordinate(&from), coordinate(&to)],
            outlines: areas
                .outlines
                .iter()
                .map(|outline| outline.iter().map(coordinate).collect())
                .collect(),
            smallest_m: areas.smallest_m,
            largest_m: areas.largest_m,
            is_inside: fai::is_fai(from, to, here),
        })
    }

    /// Hands the session the height of the ground. From then on the
    /// cockpit knows how high over the ground the glider is wherever the
    /// terrain covers it, airspace whose limits are given above ground is
    /// placed exactly there, and the side view draws the ground ahead.
    /// The terrain may gain tiles while it is in use. `None` takes it away.
    pub fn use_terrain(&self, terrain: Option<Arc<Terrain>>) {
        let mut inner = self.lock();
        inner.terrain = terrain;
        inner.ground_m = None;
    }

    /// Hands the session the airspace to warn about: the set from
    /// [`crate::Core::airspace`]. From then on every position is checked
    /// against it. `None` switches the warnings off.
    pub fn use_airspace(&self, airspace: Option<Arc<AirspaceSet>>) {
        let mut inner = self.lock();
        match (inner.watch.as_mut(), airspace) {
            // Other files in the middle of a session: the airspace is
            // exchanged, and what the pilot silenced stays silenced.
            (Some(watch), Some(set)) => watch.replace(set),
            (_, set) => inner.watch = set.map(AirspaceWatch::new),
        }
    }

    /// Whether the glider is being checked against airspace right now.
    /// Anything but `Checking` means that silence is not "no airspace
    /// near", and the screen must say which it is.
    pub fn airspace_check(&self, now_unix_s: f64) -> AirspaceCheck {
        self.lock()
            .watch
            .as_ref()
            .map_or(AirspaceCheck::NoAirspace, |watch| watch.check(now_unix_s))
    }

    /// The airspace warnings as of the latest position, the most urgent
    /// first, at most one per airspace. Empty without airspace.
    pub fn airspace_warnings(&self) -> Vec<AirspaceWarning> {
        self.lock()
            .watch
            .as_ref()
            .map(AirspaceWatch::warnings)
            .unwrap_or_default()
    }

    /// The airspace and the ground on the line ahead, seen from the side,
    /// for planning a glide: on the glider's course while it glides,
    /// towards the place the cockpit navigates to while it circles or
    /// stands. The line drawn is the glide of the last seconds on a glide,
    /// and otherwise the glider's own glide in the wind as last worked
    /// out. `None` with neither airspace nor terrain, without a position
    /// and a height, and when there is no way to look.
    pub fn airspace_ahead(&self, now_unix_s: f64) -> Option<AirspaceAhead> {
        let inner = self.lock();
        // Something to draw: airspace, or the ground.
        if inner.watch.is_none() && inner.terrain.is_none() {
            return None;
        }
        let state = inner.computer.state(now_unix_s);
        let (position, altitude_m) = (state.position?, state.altitude_m?);

        let on_course = state.course_deg.filter(|_| !state.circling).filter(|_| {
            state
                .ground_speed_ms
                .is_some_and(|speed| speed >= MOVING_MS)
        });
        let (bearing_deg, toward) = match on_course {
            Some(course) => (course, format!("On course {}", compass(course))),
            None => {
                let (target, _) = aim_of(&inner, &state)?;
                if position.distance_m(target.position) < 50.0 {
                    return None;
                }
                (
                    venti_airspace::bearing_deg(position, target.position),
                    format!("Towards {}", target.name),
                )
            }
        };

        // The line: what the glider is doing on a glide, what it can do
        // otherwise.
        let measured = on_course
            .and(state.glide_ratio)
            .filter(|glide| AHEAD_GLIDE.contains(glide));
        let (glide, line_words) = match measured {
            Some(glide) => (
                Some(glide),
                format!("Your glide of the last seconds, {glide:.1}"),
            ),
            None => {
                let there = venti_airspace::destination(position, bearing_deg, ahead::AHEAD_M)?;
                let expected = navigation::navigate(
                    position,
                    Some(altitude_m),
                    state.wind,
                    &Target {
                        name: String::new(),
                        position: there,
                        elevation_m: None,
                    },
                    &inner.performance(),
                )
                .expected_glide
                .filter(|glide| glide.is_finite() && *glide > 0.0);
                match (expected, state.wind) {
                    (Some(glide), Some(_)) => (
                        Some(glide),
                        format!("The glider's own glide in this wind, {glide:.1}"),
                    ),
                    (Some(glide), None) => (
                        Some(glide),
                        format!("The glider's own glide in still air, {glide:.1}"),
                    ),
                    (None, _) => (
                        None,
                        "The wind lets the glider make no way on this line".to_owned(),
                    ),
                }
            }
        };

        let ground = inner.terrain.as_ref().map_or_else(Vec::new, |terrain| {
            terrain.profile(
                position,
                bearing_deg,
                ahead::GROUND_STEP_M,
                ahead::GROUND_PLACES,
            )
        });
        ahead::ahead(
            inner.watch.as_ref().map(AirspaceWatch::set),
            &ahead::Look {
                position,
                altitude_m,
                qnh_altitude_m: state.qnh_altitude_m,
                pressure_altitude_m: state.pressure_altitude_m,
                bearing_deg,
                toward,
                glide,
                line_words,
                ground,
            },
        )
    }

    /// The level to sound a warning for, once: the most urgent warning
    /// that appeared, became more urgent or came out of a silence since
    /// this was last asked, and that the pilot has not silenced.
    pub fn take_airspace_alert(&self) -> Option<AirspaceLevel> {
        self.lock()
            .watch
            .as_mut()
            .and_then(AirspaceWatch::take_alert)
    }

    /// "I have seen it": the warning stays on screen and is quiet for
    /// this many minutes, unless it becomes more urgent.
    pub fn acknowledge_airspace(&self, id: String, minutes: u32) {
        if let Some(watch) = self.lock().watch.as_mut() {
            watch.acknowledge(&id, minutes);
        }
    }

    /// "This airspace does not apply to me today", for an area known to
    /// be inactive or one the pilot has permission for: quiet at every
    /// level until midnight. `utc_offset_s` is the local time zone's
    /// offset from UTC. The app cannot check the claim.
    pub fn ignore_airspace_today(&self, id: String, utc_offset_s: i32) {
        if let Some(watch) = self.lock().watch.as_mut() {
            watch.ignore_today(&id, utc_offset_s);
        }
    }

    /// Takes back an acknowledgement or an ignore.
    pub fn unsilence_airspace(&self, id: String) {
        if let Some(watch) = self.lock().watch.as_mut() {
            watch.unsilence(&id);
        }
    }

    /// Hands the session the cables to watch for: the set read from the
    /// server's answer for the flight's area, loaded before the flight as
    /// the airspace is. `None` takes them away. What the pilot silenced
    /// stays silenced.
    pub fn use_cables(&self, cables: Option<Arc<CableSet>>) {
        let mut inner = self.lock();
        inner.cables.set = cables;
        inner.cables.warning = None;
    }

    /// The cable ahead as of the latest position, by the engine's rule
    /// (`venti_flight::cables`). `None` without cables or with none ahead,
    /// which is no promise: the data is incomplete.
    pub fn cable_warning(&self) -> Option<CableWarning> {
        self.lock().cables.warning.clone()
    }

    /// Whether to sound the cable warning, once: true when a cable came
    /// ahead that was not ahead at the last ask, or came out of a silence,
    /// and the pilot has not silenced it. Sounded as the airspace warning
    /// of the level `Entering` is.
    pub fn take_cable_alert(&self) -> bool {
        let mut inner = self.lock();
        let cables = &mut inner.cables;
        let Some(warning) = cables.warning.as_ref().filter(|warning| !warning.silenced) else {
            return false;
        };
        if cables.alerted.as_deref() == Some(warning.id.as_str()) {
            return false;
        }
        cables.alerted = Some(warning.id.clone());
        true
    }

    /// "I have seen it": the cable's warning stays on screen and is quiet
    /// for this many minutes.
    pub fn acknowledge_cable(&self, id: String, minutes: u32) {
        let mut inner = self.lock();
        let cables = &mut inner.cables;
        let until = cables.now_unix_s + f64::from(minutes) * 60.0;
        cables.silenced.retain(|(silenced, _)| *silenced != id);
        if let Some(warning) = cables.warning.as_mut().filter(|warning| warning.id == id) {
            warning.silenced = true;
        }
        cables.silenced.push((id, until));
    }

    /// Chooses the place to navigate to, or with `None` goes back to
    /// navigating to the take-off.
    pub fn go_to(&self, place: Option<TargetPlace>) {
        self.lock().target = place.and_then(|place| {
            Some(Target {
                name: place.name,
                position: GeoPoint::new(place.position.lat, place.position.lon).ok()?,
                elevation_m: place.elevation_m.filter(|elevation| elevation.is_finite()),
            })
        });
    }

    /// How each of some places lies within the glider's reach, in the
    /// order given. Empty until there is a position.
    pub fn reach(&self, places: Vec<TargetPlace>, now_unix_s: f64) -> Vec<ReachView> {
        let inner = self.lock();
        let state = inner.computer.state(now_unix_s);
        let Some(position) = state.position else {
            return Vec::new();
        };
        places
            .iter()
            .map(|place| {
                reach_of(
                    place,
                    position,
                    &state,
                    &inner.performance(),
                    inner.terrain.as_deref(),
                )
            })
            .collect()
    }

    /// How far the glider glides, for a line on the map: see
    /// [`GlideRange`]. `None` on the ground, without terrain under the
    /// glider, and when it is lower over the ground than the reserve a
    /// place within reach is left.
    pub fn glide_range(&self, now_unix_s: f64) -> Option<GlideRange> {
        let inner = self.lock();
        let state = inner.computer.state(now_unix_s);
        if state.phase != Phase::Flying {
            return None;
        }
        let (position, altitude_m) = (state.position?, state.altitude_m?);
        let edge = inner.terrain.as_ref()?.with(|terrain| {
            live::range::range(
                terrain,
                position,
                altitude_m,
                state.wind,
                &inner.performance(),
                REACH_RESERVE_M,
            )
        })?;
        let mut outline: Vec<Coordinate> = edge
            .iter()
            .map(|point| Coordinate {
                lat: point.position.lat(),
                lon: point.position.lon(),
            })
            .collect();
        outline.extend(outline.first().copied());
        let distances = || edge.iter().map(|point| point.distance_m);
        Some(GlideRange {
            outline,
            reserve_m: REACH_RESERVE_M,
            nearest_m: distances().fold(f64::INFINITY, f64::min),
            farthest_m: distances().fold(0.0, f64::max),
            is_cut_short: edge
                .iter()
                .any(|point| point.end == live::range::EdgeEnd::Unknown),
        })
    }

    /// The landing to think of first, among the places given: the one the
    /// glider is expected to arrive highest above. `None` on the ground,
    /// without a position, and when none of the places has a height.
    pub fn landing_option(
        &self,
        landings: Vec<TargetPlace>,
        now_unix_s: f64,
    ) -> Option<LandingOption> {
        let inner = self.lock();
        let state = inner.computer.state(now_unix_s);
        if state.phase != Phase::Flying {
            return None;
        }
        let position = state.position?;
        let reaches: Vec<ReachView> = landings
            .iter()
            .map(|place| {
                reach_of(
                    place,
                    position,
                    &state,
                    &inner.performance(),
                    inner.terrain.as_deref(),
                )
            })
            .collect();
        // The highest arrival among the places with nothing in the way;
        // only when every one has a ridge before it, the highest of those.
        let highest = |clear_only: bool| {
            reaches
                .iter()
                .enumerate()
                .filter(|(_, reach)| reach.arrival_height_m.is_some())
                .filter(|(_, reach)| !clear_only || reach.ground_in_the_way_m.is_none())
                .max_by(|a, b| {
                    a.1.arrival_height_m
                        .unwrap_or(f64::MIN)
                        .total_cmp(&b.1.arrival_height_m.unwrap_or(f64::MIN))
                })
        };
        let (index, best) = highest(true).or_else(|| highest(false))?;
        Some(LandingOption {
            index: u32::try_from(index).ok()?,
            name: landings[index].name.clone(),
            reach: *best,
            in_reach: u32::try_from(
                reaches
                    .iter()
                    .filter(|reach| matches!(reach.reach, Reach::Within | Reach::Just))
                    .count(),
            )
            .unwrap_or(u32::MAX),
        })
    }

    /// Sets the task to fly, from its file's text as the core keeps it
    /// ([`crate::Core::kept_task_file`]), or with `None` flies no task. The
    /// cockpit then navigates to the task's next turnpoint unless the
    /// pilot chose another place. `now_unix_s` says which day the task's
    /// times of day mean.
    pub fn fly_task(&self, file_text: Option<String>, now_unix_s: f64) -> Result<(), CoreError> {
        let task = file_text
            .map(|text| TaskFlown::begin(&text, now_unix_s as i64))
            .transpose()?;
        self.lock().task = task;
        Ok(())
    }

    /// Takes in the server's answer about others in the air around the
    /// glider, as [`crate::live_others`] read it from the path that
    /// [`crate::cockpit_others_path`] gave. `now_unix_s` is when the
    /// answer came. The session keeps each aircraft's track of the last
    /// five minutes from the answers it is given.
    pub fn on_others(&self, others: LiveOthers, now_unix_s: f64) {
        self.lock().others.take(others, now_unix_s);
    }

    /// The aircraft of other networks around the glider for the cockpit's
    /// map, nearest first, each with its track of the last five minutes.
    /// Empty until there is a position and an answer.
    pub fn others_near(&self, now_unix_s: f64) -> OthersNear {
        let inner = self.lock();
        let state = inner.computer.state(now_unix_s);
        inner
            .others
            .near(state.position, state.altitude_m, now_unix_s)
    }

    /// Takes the climbs of the last minutes around the glider: the
    /// answer [`crate::live_climbs`] read, for the path
    /// [`crate::cockpit_climbs_path`] gave. `now_unix_s` is when the
    /// answer came.
    pub fn on_climbs(&self, climbs: crate::LiveClimbs, now_unix_s: f64) {
        self.lock().climbs.take(climbs, now_unix_s);
    }

    /// Where others circle and climb near the glider, for the cockpit's
    /// map: the nearest first, each with its direction, distance and age.
    /// Empty until there is a position and an answer. A hint, not an
    /// instrument: a climb found minutes ago may be gone.
    pub fn climbs_near(&self, now_unix_s: f64) -> ClimbsNear {
        let inner = self.lock();
        let state = inner.computer.state(now_unix_s);
        inner
            .climbs
            .near(state.position, state.altitude_m, now_unix_s)
    }

    /// The task as the cockpit shows it. `None` without a task, and until
    /// there is a position.
    pub fn task_status(&self, now_unix_s: f64) -> Option<TaskStatusView> {
        let inner = self.lock();
        let state = inner.computer.state(now_unix_s);
        inner
            .task
            .as_ref()?
            .view(now_unix_s, state.altitude_m, state.ground_speed_ms)
    }

    /// The next thing that happened in the task and was not yet said: the
    /// start taken, a turnpoint reached, the goal. The cockpit says each
    /// once.
    pub fn take_task_event(&self) -> Option<TaskEventView> {
        self.lock().task.as_mut()?.take_event()
    }

    /// Moves the task on to its next turnpoint without this one being
    /// reached, or with `forward` false back to the one before: for a
    /// pilot who skips a turnpoint, or goes back for a later start.
    pub fn step_task(&self, forward: bool) {
        if let Some(task) = self.lock().task.as_mut() {
            task.step(forward);
        }
    }

    /// Says how the pilot's glider flies on a straight glide in still air:
    /// its speed with the brakes up, and its glide ratio. Predictions of
    /// arrival heights use these. Values that make no sense are ignored.
    pub fn set_glider(&self, trim_speed_ms: f64, glide_ratio: f64) {
        if (5.0..=25.0).contains(&trim_speed_ms) && (3.0..=15.0).contains(&glide_ratio) {
            self.lock().glider = Glider {
                trim_speed_ms,
                glide_ratio,
            };
        }
    }

    /// Sets the glider's polar from its class, for a pilot who wants
    /// netto and a speed to fly: [`CockpitView::netto_ms`] and
    /// [`CockpitView::speed_to_fly`] are worked out from then on, and
    /// glides (arrival heights, the glide range, a task's altitude
    /// needed) are flown at the speed that carries furthest in the wind
    /// in place of trim speed. `None` takes the polar away, which is how
    /// a session begins: nothing of it shows unless the pilot asks. The
    /// classes' numbers are the project's own, to verify with pilots
    /// (`venti_flight::live::polar`).
    pub fn set_glider_class(&self, class: Option<GliderClassKind>) {
        self.lock().polar = class.map(|class| {
            Polar::for_class(match class {
                GliderClassKind::A => GliderClass::A,
                GliderClassKind::B => GliderClass::B,
                GliderClassKind::C => GliderClass::C,
                GliderClassKind::D => GliderClass::D,
                GliderClassKind::Ccc => GliderClass::Ccc,
            })
        });
    }

    /// Sets the glider's polar from the pilot's own numbers: trim speed
    /// with the sink at it and top speed with the sink at it, metres per
    /// second, the best glide in still air taken to be at trim. Returns
    /// whether they make a polar; if not, nothing changes.
    pub fn set_polar(
        &self,
        trim_speed_ms: f64,
        trim_sink_ms: f64,
        max_speed_ms: f64,
        max_sink_ms: f64,
    ) -> bool {
        let polar =
            Polar::from_trim_and_max(trim_speed_ms, trim_sink_ms, max_speed_ms, max_sink_ms);
        if let Some(polar) = polar {
            self.lock().polar = Some(polar);
        }
        polar.is_some()
    }

    /// Hands the session the landing fields to keep a standing reading
    /// of ([`CockpitView::nearest_landing`]): the landings among the
    /// sites the app holds around the glider, and the pilot's own. A
    /// place without a height takes the terrain's. An empty list takes
    /// the reading away.
    pub fn use_landings(&self, landings: Vec<TargetPlace>) {
        let mut inner = self.lock();
        inner.landings = landings
            .into_iter()
            .filter_map(|place| {
                Some(Target {
                    name: place.name,
                    position: GeoPoint::new(place.position.lat, place.position.lon).ok()?,
                    elevation_m: place.elevation_m.filter(|elevation| elevation.is_finite()),
                })
            })
            .collect();
        inner.landing = None;
    }

    /// For hike and fly: with `true` the flight is never taken for
    /// landed, so that landing, packing, walking on and taking off again
    /// stay one recording. The pilot then ends the session by hand.
    pub fn set_never_land(&self, never_land: bool) {
        let mut inner = self.lock();
        inner.never_land = never_land;
        inner.computer.set_never_land(never_land);
    }

    /// Sets the QNH by hand, hectopascals; `None` goes back to the QNH
    /// taken on the ground before take-off. A value no sea level has had
    /// is ignored.
    pub fn set_qnh(&self, qnh_hpa: Option<f64>) {
        let mut inner = self.lock();
        inner.qnh_by_hand = qnh_hpa.filter(|qnh| atmosphere::QNH_HPA.contains(qnh));
        let by_hand = inner.qnh_by_hand;
        inner.computer.set_qnh(by_hand);
    }

    /// The way to the chosen place, or to the take-off when none is
    /// chosen. `None` until there is a position, and on the ground before
    /// a take-off when no place is chosen.
    pub fn navigation(&self, now_unix_s: f64) -> Option<WayToTarget> {
        let inner = self.lock();
        let state = inner.computer.state(now_unix_s);
        let position = state.position?;
        let (target, is_takeoff) = aim_of(&inner, &state)?;
        let way = navigation::navigate(
            position,
            state.altitude_m,
            state.wind,
            &target,
            &inner.performance(),
        );
        let ground_in_the_way_m = ground_in_the_way(
            inner.terrain.as_deref(),
            position,
            state.altitude_m,
            &way,
            target.elevation_m,
        );
        Some(WayToTarget {
            name: way.name,
            is_takeoff,
            position: Coordinate {
                lat: target.position.lat(),
                lon: target.position.lon(),
            },
            distance_m: way.distance_m,
            bearing_deg: way.bearing_deg,
            height_above_m: way.height_above_m,
            required_glide: way.required_glide,
            expected_glide: way.expected_glide,
            arrival_height_m: way.arrival_height_m,
            time_to_go_s: way.time_to_go_s,
            wind_known: way.wind_known,
            ground_in_the_way_m,
        })
    }

    /// Takes a position from the phone.
    pub fn on_location(&self, sample: LocationSample) {
        let Ok(position) = GeoPoint::new(sample.lat, sample.lon) else {
            return;
        };
        let mut inner = self.lock();
        // The ground first: the landing rule looks at the height over it,
        // and before take-off the QNH is taken from it.
        let ground_m = inner
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.elevation_at(position));
        inner.computer.ground(sample.time_unix_s, ground_m);
        inner.computer.position(PositionSample {
            time_s: sample.time_unix_s,
            position,
            altitude_m: sample.altitude_m,
            ground_speed_ms: sample.ground_speed_ms.filter(|speed| *speed >= 0.0),
            course_deg: sample.course_deg.filter(|course| *course >= 0.0),
        });
        inner.last_position_s = Some(sample.time_unix_s);

        let state = inner.computer.state(sample.time_unix_s);
        let pressure_altitude = state.pressure_altitude_m;
        if state.phase == Phase::Flying {
            let due = inner.flown.last().is_none_or(|last| {
                sample.time_unix_s - last.time.timestamp() as f64 >= SCORE_FIX_EVERY_S
            });
            if due && inner.flown.len() < SCORE_FIXES_AT_MOST {
                // The longest straight line flown can only grow by a line
                // that ends at the newest fix.
                let reach = inner
                    .flown
                    .iter()
                    .map(|fix| (fix.position, fix.position.distance_m(position)))
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((from, distance)) = reach
                    && inner
                        .longest
                        .is_none_or(|(_, _, longest)| distance > longest)
                {
                    inner.longest = Some((from, position, distance));
                }
                let altitude = state.altitude_m.or(sample.altitude_m).unwrap_or(0.0);
                inner.flown.push(Fix {
                    time: DateTime::from_timestamp(sample.time_unix_s as i64, 0)
                        .unwrap_or_default(),
                    position,
                    gps_altitude_m: altitude.round() as i32,
                    pressure_altitude_m: 0,
                    valid: true,
                });
            }
        }
        if let Some(task) = inner.task.as_mut() {
            task.feed(sample.time_unix_s, position);
        }
        // What others may watch: from take-off, and the place the glider
        // came to rest at.
        let replayed = inner.replayed;
        if let Some(share) = inner.live.as_mut().filter(|_| !replayed)
            && state.phase != Phase::Ground
        {
            let altitude = state.altitude_m.or(sample.altitude_m).unwrap_or(0.0);
            if let Some(fix) = sharing::fix(
                sample.time_unix_s,
                position,
                altitude,
                state.ground_speed_ms,
                state.climb_ms,
                state.course_deg,
            ) {
                share.add(fix);
            }
            share.set_landed(state.phase == Phase::Landed);
        }
        inner.ground_m = ground_m;
        inner.landing = nearest_landing_reading(&inner, position, &state);
        // The glide the cable rule goes by: the worse of the glide the
        // glider makes now and its own in this wind on this course.
        let own_glide = state.course_deg.and_then(|course| {
            navigation::glide_over_ground(course, state.wind, &inner.performance())
        });
        let cable_glide = match (state.glide_ratio, own_glide) {
            (Some(now), Some(own)) => Some(now.min(own)),
            (now, own) => now.or(own),
        };
        inner.cables.feed(
            sample.time_unix_s,
            position,
            state
                .altitude_m
                .zip(ground_m)
                .map(|(up, ground)| up - ground),
            state.course_deg,
            state.ground_speed_ms,
            cable_glide,
            state.turn_rate_deg_s,
        );
        if let Some(watch) = inner.watch.as_mut() {
            // Two altitudes: the one height over the terrain is measured
            // with, for the ground and limits above it, and the one at the
            // QNH, for limits above sea level, when there is a QNH.
            watch.feed(
                sample.time_unix_s,
                position,
                state.altitude_m,
                state.qnh_altitude_m,
                state.pressure_altitude_m,
                ground_m,
                state.ground_speed_ms,
                state.course_deg,
                state.average_climb_ms,
            );
        }
        let inner = &mut *inner;
        if let Some(recorder) = inner.recorder.as_mut() {
            match inner.lead.as_mut() {
                // Waiting for the take-off: kept, not written.
                Some(lead) if state.phase == Phase::Ground => {
                    lead.keep(sample, position, pressure_altitude);
                }
                // A failed write must not stop the flight computer. The fix
                // is lost; the next one tries again.
                _ => {
                    // The take-off: the file begins with what was kept.
                    if let Some(lead) = inner.lead.take() {
                        for (kept, position, pressure_altitude) in lead.fixes {
                            let _ = recorder.add(&kept, position, pressure_altitude);
                        }
                    }
                    let _ = recorder.add(&sample, position, pressure_altitude);
                }
            }
        }
        // A hop too short to be a flight: what was written of it is
        // thrown away, and the session waits for the take-off again.
        let hop = state
            .takeoff_time_s
            .zip(state.landing_time_s)
            .is_some_and(|(takeoff, landing)| landing - takeoff < SHORTEST_FLIGHT_S);
        if hop && state.phase == Phase::Landed && inner.by_itself && !inner.by_hand {
            inner.wait_again(sample.time_unix_s);
        }
    }

    /// Takes a pressure reading from the phone's own sensor, in
    /// hectopascals. Ignored while an external instrument is supplying
    /// pressure: the instrument's sensor is the better one.
    pub fn on_pressure(&self, time_unix_s: f64, pressure_hpa: f64) {
        let mut inner = self.lock();
        let instrument_has_it = inner
            .instrument
            .and_then(|instrument| instrument.last_pressure_s)
            .is_some_and(|heard| time_unix_s - heard <= INSTRUMENT_SILENCE_S);
        if !instrument_has_it {
            inner.pressure_altitude(
                PressureSource::Phone,
                time_unix_s,
                atmosphere::pressure_altitude_m(pressure_hpa),
            );
        }
    }

    /// Takes bytes from an external instrument, exactly as Bluetooth
    /// delivered them.
    pub fn on_instrument_bytes(&self, time_unix_s: f64, bytes: Vec<u8>) {
        let mut inner = self.lock();
        let lines = inner.reader.push(&bytes);
        let mut instrument = inner.instrument.unwrap_or(Instrument {
            last_heard_s: time_unix_s,
            last_pressure_s: None,
            battery_percent: None,
            sentences: 0,
            damaged: 0,
        });
        for line in lines {
            let Ok(line) = line else {
                instrument.damaged += 1;
                continue;
            };
            let Some(reading) = venti_sensors::parse(&line) else {
                continue;
            };
            instrument.sentences += 1;
            instrument.last_heard_s = time_unix_s;
            let altitude = reading
                .pressure_hpa
                .map(atmosphere::pressure_altitude_m)
                .or(reading.pressure_altitude_m);
            if let Some(altitude) = altitude {
                inner.pressure_altitude(PressureSource::Instrument, time_unix_s, altitude);
                instrument.last_pressure_s = Some(time_unix_s);
            }
            if let Some(climb) = reading.vario_ms {
                inner.computer.instrument_vario(time_unix_s, climb);
            }
            if let Some(Battery::Percent(percent)) = reading.battery {
                instrument.battery_percent = Some(percent);
            }
        }
        if instrument.sentences > 0 || instrument.damaged > 0 {
            inner.instrument = Some(instrument);
        }
    }

    /// Starts writing fixes to a recording, from now. Does nothing if one
    /// is running; one that waits for the take-off waits no longer, and
    /// begins with the half minute it has kept. The pilot's and the
    /// glider's names go into the flight file's header, if given.
    pub fn start_recording(
        &self,
        pilot: Option<String>,
        glider: Option<String>,
    ) -> Result<(), CoreError> {
        let mut inner = self.lock();
        let inner = &mut *inner;
        if inner.recorder.is_none() {
            let recorder = Recorder::new(&inner.folder, header_text(pilot), header_text(glider));
            inner.recorder = Some(recorder.map_err(storage)?);
        }
        if let (Some(lead), Some(recorder)) = (inner.lead.take(), inner.recorder.as_mut()) {
            for (kept, position, pressure_altitude) in lead.fixes {
                let _ = recorder.add(&kept, position, pressure_altitude);
            }
        }
        inner.by_hand = true;
        Ok(())
    }

    /// Makes the flight record itself. Nothing is written while the glider
    /// is on the ground; when it takes off the flight file begins, with the
    /// last half minute before. From then the view says when the session
    /// is over: a while after the landing, or when nobody has taken off for
    /// two hours. Does nothing if a recording is running or waiting.
    pub fn record_from_takeoff(
        &self,
        pilot: Option<String>,
        glider: Option<String>,
        now_unix_s: f64,
    ) -> Result<(), CoreError> {
        let mut inner = self.lock();
        if inner.recorder.is_some() {
            return Ok(());
        }
        let recorder = Recorder::new(&inner.folder, header_text(pilot), header_text(glider));
        inner.recorder = Some(recorder.map_err(storage)?);
        inner.by_itself = true;
        // A session switched on in the air records at once.
        if inner.computer.state(now_unix_s).phase == Phase::Ground {
            inner.lead = Some(Lead {
                since_s: now_unix_s,
                fixes: VecDeque::new(),
            });
        }
        Ok(())
    }

    /// Stops recording and returns the flight file's text, or nothing if no
    /// fix was recorded. The file stays in the data folder until
    /// [`clear_recording`] is called, so the text is not lost if importing
    /// it fails.
    pub fn stop_recording(&self) -> Result<Option<PendingRecording>, CoreError> {
        let recorder = {
            let mut inner = self.lock();
            inner.lead = None;
            inner.by_itself = false;
            inner.by_hand = false;
            inner.recorder.take()
        };
        let Some(recorder) = recorder else {
            return Ok(None);
        };
        recorder.finish().map_err(storage)
    }

    /// What the cockpit shows at `now_unix_s`.
    pub fn view(&self, now_unix_s: f64) -> CockpitView {
        let inner = self.lock();
        let state = inner.computer.state(now_unix_s);
        let coordinate = |point: GeoPoint| Coordinate {
            lat: point.lat(),
            lon: point.lon(),
        };
        let end = state.landing_time_s.unwrap_or(now_unix_s);
        // A flight that records itself is saved a while after the landing.
        let saves_in_s = state
            .landing_time_s
            .filter(|_| inner.by_itself && state.phase == Phase::Landed)
            .map(|landed| (landed + SAVE_AFTER_LANDING_S - now_unix_s).max(0.0).ceil() as i64);
        let waited_s = inner.lead.as_ref().map(|lead| now_unix_s - lead.since_s);
        // The glider through the air, where the wind is fresh enough to
        // say it from.
        let wind_age_s = state
            .wind_measured_s
            .map(|measured| (now_unix_s - measured).max(0.0));
        let air = state
            .air
            .filter(|_| state.phase == Phase::Flying)
            .filter(|_| wind_age_s.is_some_and(|age| age <= STRONG_WIND_FRESH_S));
        // Netto and the speed to fly: only with a polar, and only on a
        // glide that is not turning.
        let gliding = !state.circling
            && state
                .turn_rate_deg_s
                .is_none_or(|rate| rate.abs() < NETTO_TURN_AT_MOST_DEG_S);
        let netto = inner
            .polar
            .zip(air)
            .zip(state.climb_ms)
            .filter(|_| gliding)
            .map(|((polar, air), climb)| (polar, air, polar.netto_ms(climb, air.airspeed_ms)));
        let over = if saves_in_s == Some(0) {
            Some(SessionOver::Landed)
        } else if waited_s.is_some_and(|waited| waited >= WAIT_FOR_TAKEOFF_S) {
            Some(SessionOver::NoTakeoff)
        } else {
            None
        };

        CockpitView {
            phase: match state.phase {
                Phase::Ground => FlightPhase::Ground,
                Phase::Flying => FlightPhase::Flying,
                Phase::Landed => FlightPhase::Landed,
            },
            recording: inner.recorder.is_some() && inner.lead.is_none(),
            waiting_for_takeoff: inner.lead.is_some(),
            over,
            saves_in_s,
            recorded_fixes: inner.recorder.as_ref().map_or(0, |recorder| recorder.fixes),
            flight_time_s: state
                .takeoff_time_s
                .map(|takeoff| (end - takeoff).max(0.0) as i64),
            position_age_s: inner
                .last_position_s
                .map(|last| (now_unix_s - last).max(0.0)),
            position: state.position.map(coordinate),
            altitude_m: state.altitude_m,
            height_above_takeoff_m: state.height_above_takeoff_m,
            ground_elevation_m: inner.ground_m,
            height_above_ground_m: state
                .altitude_m
                .zip(inner.ground_m)
                .map(|(altitude, ground)| altitude - ground),
            max_altitude_m: state.max_altitude_m,
            climb_ms: state.climb_ms,
            vario_source: state.vario_source.map(|source| match source {
                VarioSource::Instrument => VarioSourceKind::Instrument,
                VarioSource::Pressure => VarioSourceKind::Pressure,
                VarioSource::Satellite => VarioSourceKind::Satellite,
            }),
            average_climb_ms: state.average_climb_ms,
            ground_speed_ms: state.ground_speed_ms,
            course_deg: state.course_deg,
            course_words: state
                .course_deg
                .zip(state.ground_speed_ms)
                .filter(|(course, speed)| course.is_finite() && *speed >= MOVING_MS)
                .map(|(course, _)| course_in_words(course)),
            glide_ratio: state.glide_ratio,
            wind: state.wind.map(|wind| WindView {
                from_deg: wind.from_deg,
                speed_ms: wind.speed_ms,
            }),
            wind_age_s,
            strong_wind: state
                .wind
                .zip(state.wind_measured_s)
                .and_then(|(wind, measured)| {
                    strong_wind_in_words(
                        inner.glider.trim_speed_ms,
                        wind.speed_ms,
                        now_unix_s - measured,
                    )
                }),
            winds_by_height: state
                .winds
                .iter()
                .map(|band| WindAtHeightView {
                    floor_m: band.floor_m,
                    ceiling_m: band.floor_m + live::WIND_BAND_M,
                    from_deg: band.wind.from_deg,
                    speed_ms: band.wind.speed_ms,
                    age_s: (now_unix_s - band.measured_s).max(0.0),
                    is_here: state.altitude_m.is_some_and(|altitude| {
                        (band.floor_m..band.floor_m + live::WIND_BAND_M).contains(&altitude)
                    }),
                    climb_ms: band.climb_ms,
                })
                .collect(),
            circling: state.circling,
            thermal: state.thermal.map(|thermal| ThermalView {
                gain_m: thermal.gain_m,
                climb_ms: thermal.climb_ms,
            }),
            lift: state.lift.as_ref().map(lift_view),
            thermals: state
                .thermals
                .iter()
                .map(|mark| ThermalMarkView {
                    position: coordinate(mark.centre),
                    climb_ms: mark.climb_ms,
                    gain_m: mark.gain_m,
                    top_altitude_m: mark.top_altitude_m,
                    label: format!("{:+.1}", mark.climb_ms),
                })
                .collect(),
            takeoff: state.takeoff.map(coordinate),
            distance_to_takeoff_m: state.distance_to_takeoff_m,
            bearing_to_takeoff_deg: state.bearing_to_takeoff_deg,
            trail: state
                .trail
                .iter()
                .map(|point| TrailPointView {
                    lat: point.position.lat(),
                    lon: point.position.lon(),
                    climb_ms: point.climb_ms,
                })
                .collect(),
            // A vario that beeps while the pilot walks to the take-off is a
            // nuisance; it sounds only in the air.
            tone: state
                .climb_ms
                .filter(|_| state.phase == Phase::Flying)
                .and_then(tone::tone)
                .map(|tone| ToneView {
                    frequency_hz: tone.frequency_hz,
                    cycle_s: tone.cycle_s,
                    duty: tone.duty,
                }),
            instrument: inner.instrument.map(|instrument| InstrumentView {
                talking: now_unix_s - instrument.last_heard_s <= INSTRUMENT_SILENCE_S,
                battery_percent: instrument.battery_percent,
                sentences: instrument.sentences,
                damaged: instrument.damaged,
            }),
            airspace_proximity: inner.watch.as_ref().and_then(AirspaceWatch::proximity),
            task_readings: inner.task.as_ref().and_then(|task| {
                task.readings(
                    now_unix_s,
                    state.altitude_m,
                    state.ground_speed_ms,
                    state.circling,
                    state.wind,
                    &inner.performance(),
                )
            }),
            wind_source: state.wind_estimate.map(|estimate| match estimate.source {
                WindSource::Circling => WindSourceKind::Circling,
                WindSource::Glide => WindSourceKind::Glide,
            }),
            wind_words: state
                .wind_estimate
                .map(|estimate| wind_in_words(&estimate, now_unix_s)),
            airspeed_ms: air.map(|air| air.airspeed_ms),
            airspeed_words: air.map(|air| format!("about {:.0} km/h", air.airspeed_ms * 3.6)),
            heading_deg: air.map(|air| air.heading_deg),
            heading_words: air.map(|air| format!("about {}", course_in_words(air.heading_deg))),
            carried_back: air
                .filter(|_| !state.circling)
                .and_then(|air| air.backwards_ms)
                .map(carried_back_in_words),
            nearest_landing: inner
                .landing
                .clone()
                .filter(|_| state.phase == Phase::Flying),
            netto_ms: netto.map(|(_, _, netto)| netto),
            netto_words: netto
                .map(|(_, _, netto)| format!("air {netto:+.1} m/s").replace('-', "−")),
            speed_to_fly: netto.map(|(polar, air, netto)| {
                speed_to_fly_view(&polar.speed_to_fly(air.head_wind_ms, netto, 0.0))
            }),
            altitude_source_words: state.altitude_source.map(|source| {
                match source {
                    AltitudeSource::Pressure => {
                        "Altitude from the pressure sensor, levelled to GPS"
                    }
                    AltitudeSource::Satellite => "Altitude from GPS",
                }
                .to_owned()
            }),
            qnh_hpa: state.qnh.map(|(qnh, _)| qnh),
            qnh_altitude_m: state.qnh_altitude_m,
            qnh_words: state.qnh.map(|(qnh, source)| {
                format!(
                    "QNH {qnh:.0} hPa, {} · {}",
                    match source {
                        QnhSource::Ground => "from the ground at take-off",
                        QnhSource::Hand => "set by hand",
                    },
                    // What the airspace warnings go by, as the watch is
                    // fed from the same state.
                    match state.qnh_altitude_m {
                        Some(_) => "airspace above sea level measured at it",
                        None => "no pressure now: airspace above sea level by GPS",
                    }
                )
            }),
            never_lands: inner.never_land,
        }
    }
}

/// Netto and the speed to fly are given while the course turns slower
/// than this, degrees per second: in a turn a glider sinks faster than
/// its polar says.
const NETTO_TURN_AT_MOST_DEG_S: f64 = 3.0;

/// From this share of the speed bar on the hint is to speed up.
const SPEED_UP_FROM_BAR: f64 = 0.25;

/// "11 km/h from W, from your last glide, 2 min old".
fn wind_in_words(estimate: &live::WindEstimate, now_unix_s: f64) -> String {
    let age_s = (now_unix_s - estimate.measured_s).max(0.0);
    let minutes = (age_s / 60.0).floor() as i64;
    let age = match minutes {
        0 => "under a minute old".to_owned(),
        1..=59 => format!("{minutes} min old"),
        _ => format!("{} h {:02} min old", minutes / 60, minutes % 60),
    };
    format!(
        "{:.0} km/h from {}, from your last {}, {age}",
        estimate.wind.speed_ms * 3.6,
        compass(estimate.wind.from_deg),
        match estimate.source {
            WindSource::Circling => "circles",
            WindSource::Glide => "glide",
        }
    )
}

/// "The wind carries you back: 4 km/h backwards".
fn carried_back_in_words(backwards_ms: f64) -> String {
    format!(
        "The wind carries you back: {:.0} km/h backwards",
        (backwards_ms * 3.6).round().max(1.0)
    )
}

/// The speed to fly as a hint in words.
fn speed_to_fly_view(speed: &live::polar::SpeedToFly) -> SpeedToFlyView {
    let speed_up = speed.bar >= SPEED_UP_FROM_BAR;
    let words = if speed.rising_air {
        "Trim: you are in rising air".to_owned()
    } else if speed_up {
        match (speed.sinking_air, speed.head_wind) {
            (true, true) => "Speed up: sinking air and head wind",
            (true, false) => "Speed up: sinking air",
            (false, true) => "Speed up: head wind",
            (false, false) => "Speed up",
        }
        .to_owned()
    } else if speed.tail_wind {
        "Trim: tail wind".to_owned()
    } else {
        "Trim".to_owned()
    };
    SpeedToFlyView {
        words,
        bar: speed.bar,
        speed_up,
    }
}

/// The standing reading of the nearest landing within glide, for the
/// position just taken. `None` on the ground, without landings, and
/// without a height.
fn nearest_landing_reading(
    inner: &Inner,
    position: GeoPoint,
    state: &live::LiveState,
) -> Option<LandingReading> {
    if state.phase != Phase::Flying || inner.landings.is_empty() {
        return None;
    }
    let altitude_m = state.altitude_m?;
    let find = |terrain: Option<&venti_flight::terrain::Terrain>| {
        landing::nearest_landing(
            terrain,
            position,
            altitude_m,
            state.wind,
            &inner.performance(),
            &inner.landings,
            landing::RESERVE_M,
        )
    };
    let found = match inner.terrain.as_ref() {
        Some(terrain) => terrain.with(|terrain| find(Some(terrain))),
        None => find(None),
    }?;
    let (within_glide, reach) = match found {
        NearestLanding::Within(reach) => (true, Some(reach)),
        NearestLanding::NoneWithin(reach) => (false, reach),
    };
    let name = reach.map(|reach| inner.landings[reach.index].name.clone());
    let words = match (within_glide, reach, name.as_deref()) {
        (true, Some(reach), Some(name)) => format!(
            "Landing {name}: {:.1} km, {:+.0} m",
            reach.distance_m / 1000.0,
            reach.arrival_height_m.unwrap_or(0.0)
        ),
        (_, Some(reach), Some(name)) => {
            let km = reach.distance_m / 1000.0;
            match (reach.ground_in_the_way_m, reach.arrival_height_m) {
                (Some(_), _) => {
                    format!("No landing within glide: {name} {km:.1} km, a ridge in the way")
                }
                (None, Some(arrival)) => format!(
                    "No landing within glide: {name} {km:.1} km, {:.0} m short",
                    landing::RESERVE_M - arrival
                ),
                (None, None) => {
                    format!("No landing within glide: {name} {km:.1} km, no way against the wind")
                }
            }
        }
        _ => "No landing within glide".to_owned(),
    };
    Some(LandingReading {
        within_glide,
        index: reach.and_then(|reach| u32::try_from(reach.index).ok()),
        name,
        distance_m: reach.map(|reach| reach.distance_m),
        bearing_deg: reach.map(|reach| reach.bearing_deg),
        arrival_height_m: reach.and_then(|reach| reach.arrival_height_m),
        ground_in_the_way_m: reach.and_then(|reach| reach.ground_in_the_way_m),
        words,
    })
}

/// A score as the cockpit shows it.
fn live_score(scores: &scoring::Scores, flown: &[Fix]) -> LiveScore {
    let score = &scores.best;
    // The circle that closes the best triangle, while it is open: also
    // while a free flight scores more, for a triangle that may still be
    // closed.
    let closing = scores
        .triangle
        .as_ref()
        .filter(|triangle| !triangle.closed)
        .and_then(|triangle| scoring::closing_circle(triangle, flown))
        .map(|circle| closing_circle_view(&circle, flown.last().map(|fix| fix.position)));
    LiveScore {
        kind: match score.kind {
            scoring::RouteKind::FreeFlight => RouteKind::FreeFlight,
            scoring::RouteKind::FreeTriangle => RouteKind::FreeTriangle,
            scoring::RouteKind::FaiTriangle => RouteKind::FaiTriangle,
        },
        closed: score.closed,
        distance_m: score.distance_m,
        points: score.points,
        closing_m: score.closing_m,
        route: score
            .route
            .iter()
            .filter_map(|&index| flown.get(index))
            .map(|fix| Coordinate {
                lat: fix.position.lat(),
                lon: fix.position.lon(),
            })
            .collect(),
        closing,
    }
}

impl FlightSession {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Every change to the session is complete before the lock is
        // released, so the state is usable even after a panic elsewhere.
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The recordings in the data folder that were never imported: a flight
/// that was being recorded when the app was closed or crashed. Call at
/// launch, import each, then clear it.
#[uniffi::export]
pub fn pending_recordings(data_folder: String) -> Vec<PendingRecording> {
    let Ok(entries) = fs::read_dir(recordings_folder(Path::new(&data_folder))) else {
        return Vec::new();
    };
    let mut recordings: Vec<PendingRecording> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?.to_owned();
            let igc_text = fs::read_to_string(&path).ok()?;
            // A recording with no fix yet is not a flight.
            igc_text
                .lines()
                .any(|line| line.starts_with('B'))
                .then_some(PendingRecording { name, igc_text })
        })
        .collect();
    recordings.sort_by(|a, b| a.name.cmp(&b.name));
    recordings
}

/// Removes a recording from the data folder, once it is in the logbook.
#[uniffi::export]
pub fn clear_recording(data_folder: String, name: String) -> Result<(), CoreError> {
    // The name becomes a path: accept only the form this module creates.
    let safe = name.ends_with(".igc")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.');
    if !safe {
        return Ok(());
    }
    match fs::remove_file(recordings_folder(Path::new(&data_folder)).join(name)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(storage(error)),
        _ => Ok(()),
    }
}

/// A Bluetooth service that carries an instrument's text stream.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SerialServiceView {
    /// What the service is, for diagnostics.
    pub name: String,
    /// The service's identifier, a 128-bit UUID.
    pub service: String,
    /// The characteristic on which the instrument sends its stream.
    pub notify: String,
}

/// The Bluetooth services to look for when searching for a vario, so every
/// app looks for the same devices.
#[uniffi::export]
pub fn vario_serial_services() -> Vec<SerialServiceView> {
    venti_sensors::ble::SERIAL_SERVICES
        .iter()
        .map(|service| SerialServiceView {
            name: service.name.to_owned(),
            service: service.service.to_owned(),
            notify: service.notify.to_owned(),
        })
        .collect()
}

fn recordings_folder(data_folder: &Path) -> PathBuf {
    data_folder.join("recordings")
}

fn storage(error: io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("The recording could not be written: {error}."),
    }
}

/// A name made fit for a flight file's header: the format is plain ASCII,
/// one line per field. Returns `None` if nothing usable is left.
fn header_text(name: Option<String>) -> Option<String> {
    let cleaned: String = name?
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(60)
        .collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}

/// Writes a flight file fix by fix.
#[derive(Debug)]
struct Recorder {
    folder: PathBuf,
    pilot: Option<String>,
    glider: Option<String>,
    /// The open file and its name, once the first fix has given the date.
    file: Option<(File, String)>,
    last_second: Option<i64>,
    fixes: u32,
}

impl Recorder {
    fn new(data_folder: &Path, pilot: Option<String>, glider: Option<String>) -> io::Result<Self> {
        let folder = recordings_folder(data_folder);
        fs::create_dir_all(&folder)?;
        Ok(Self {
            folder,
            pilot,
            glider,
            file: None,
            last_second: None,
            fixes: 0,
        })
    }

    /// Appends a fix. Flight files hold at most one fix a second, so
    /// further positions within the same second are skipped.
    fn add(
        &mut self,
        sample: &LocationSample,
        position: GeoPoint,
        pressure_altitude_m: Option<f64>,
    ) -> io::Result<()> {
        let second = sample.time_unix_s.floor() as i64;
        if self.last_second.is_some_and(|last| second <= last) {
            return Ok(());
        }
        let Some(time) = DateTime::from_timestamp(second, 0) else {
            return Ok(());
        };

        if self.file.is_none() {
            let name = format!("{}.igc", time.format("%Y%m%d-%H%M%S"));
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.folder.join(&name))?;
            let header = igc::write_header(&IgcFile {
                // "X" marks a recorder without an approval code of its own.
                manufacturer: "XVE".to_owned(),
                recorder_id: "001".to_owned(),
                date: time.date_naive(),
                pilot: self.pilot.clone(),
                glider_type: self.glider.clone(),
                glider_id: None,
                fixes: Vec::new(),
                skipped_lines: 0,
            });
            file.write_all(header.as_bytes())?;
            self.file = Some((file, name));
        }
        let (file, _) = self.file.as_mut().expect("the file was just opened");

        let line = igc::write_fix(&Fix {
            time,
            position,
            gps_altitude_m: sample
                .altitude_m
                .map_or(0, |altitude| altitude.round() as i32),
            pressure_altitude_m: pressure_altitude_m.map_or(0, |altitude| altitude.round() as i32),
            valid: sample.altitude_m.is_some(),
        });
        file.write_all(line.as_bytes())?;
        self.last_second = Some(second);
        self.fixes += 1;
        if self.fixes.is_multiple_of(FIXES_PER_SYNC) {
            file.sync_data()?;
        }
        Ok(())
    }

    /// Throws away what was written: the next fix begins a new file.
    fn restart(&mut self) -> io::Result<()> {
        self.last_second = None;
        self.fixes = 0;
        if let Some((file, name)) = self.file.take() {
            drop(file);
            fs::remove_file(self.folder.join(name))?;
        }
        Ok(())
    }

    /// Closes the file and returns its contents, or removes it if it holds
    /// no fix.
    fn finish(self) -> io::Result<Option<PendingRecording>> {
        let Some((file, name)) = self.file else {
            return Ok(None);
        };
        file.sync_all()?;
        drop(file);
        let igc_text = fs::read_to_string(self.folder.join(&name))?;
        Ok(Some(PendingRecording { name, igc_text }))
    }
}

/// A recorded flight played back through a session.
///
/// The shell calls [`advance`](Replay::advance) from a timer with however
/// many seconds of the flight should pass, and uses [`now`](Replay::now) as
/// the time when it asks the session for its view. Positions and pressure
/// altitudes come from the file; nothing is invented.
#[derive(Debug, uniffi::Object)]
pub struct Replay {
    fixes: Vec<Fix>,
    progress: Mutex<Progress>,
}

#[derive(Debug, Clone, Copy)]
struct Progress {
    next: usize,
    now_s: f64,
}

#[uniffi::export]
impl Replay {
    /// Prepares a flight file for playing back.
    #[uniffi::constructor]
    pub fn new(igc_text: String) -> Result<Arc<Self>, CoreError> {
        let file = igc::parse(&igc_text).map_err(crate::LogbookError::Invalid)?;
        let start = file
            .fixes
            .first()
            .map_or(0.0, |fix| fix.time.timestamp() as f64);
        Ok(Arc::new(Self {
            fixes: file.fixes,
            progress: Mutex::new(Progress {
                next: 0,
                now_s: start,
            }),
        }))
    }

    /// Lets `seconds` of the flight pass, feeding its fixes to the session.
    /// Returns false once the flight is over.
    pub fn advance(&self, session: Arc<FlightSession>, seconds: f64) -> bool {
        let mut progress = self
            .progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        progress.now_s += seconds.max(0.0);
        {
            // What is replayed is not shared with anyone.
            let mut inner = session.lock();
            inner.replayed = true;
            inner.live = None;
        }
        while let Some(fix) = self.fixes.get(progress.next) {
            let time = fix.time.timestamp() as f64;
            if time > progress.now_s {
                break;
            }
            // Recorders without a pressure sensor write zero.
            if fix.pressure_altitude_m != 0 {
                session.lock().pressure_altitude(
                    PressureSource::Replay,
                    time,
                    f64::from(fix.pressure_altitude_m),
                );
            }
            session.on_location(LocationSample {
                time_unix_s: time,
                lat: fix.position.lat(),
                lon: fix.position.lon(),
                altitude_m: fix.valid.then(|| f64::from(fix.gps_altitude_m)),
                ground_speed_ms: None,
                course_deg: None,
            });
            progress.next += 1;
        }
        progress.next < self.fixes.len()
    }

    /// The moment the replay has reached, seconds since 1970-01-01 UTC.
    pub fn now(&self) -> f64 {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .now_s
    }
}

/// The place the cockpit navigates to, and whether it is the take-off:
/// the place the pilot chose comes first, then the task, then the way
/// back to the take-off.
fn aim_of(inner: &Inner, state: &live::LiveState) -> Option<(Target, bool)> {
    let chosen = inner
        .target
        .clone()
        .or_else(|| inner.task.as_ref().and_then(TaskFlown::aim));
    Some(match chosen {
        Some(target) => (target, false),
        None => (
            Target {
                name: "Take-off".to_owned(),
                position: state.takeoff?,
                // The height the glider left the ground at.
                elevation_m: state
                    .altitude_m
                    .zip(state.height_above_takeoff_m)
                    .map(|(altitude, above)| altitude - above),
            },
            true,
        ),
    })
}

/// Below this speed over the ground a course says nothing, metres per
/// second: it is not looked along for the side view and not written as a
/// reading.
const MOVING_MS: f64 = 2.0;

/// A course as a reading: whole degrees from 0 to 359 and the compass
/// point, "285° W".
fn course_in_words(course_deg: f64) -> String {
    let degrees = course_deg.rem_euclid(360.0).round() % 360.0;
    format!("{degrees:.0}° {}", compass(course_deg))
}

/// A glide of the last seconds is taken for the line ahead only within
/// these bounds: flatter, and the glider is in lift that will not last
/// ten kilometres.
const AHEAD_GLIDE: std::ops::RangeInclusive<f64> = 1.0..=15.0;

/// A place counts as within reach when the glider is expected to arrive
/// at least this far above it, metres: room for a circuit and for the
/// prediction being a little wrong. To verify in the air.
const REACH_RESERVE_M: f64 = 100.0;

/// The ground under a glide to a place is read this far apart, metres.
const REACH_GROUND_STEP_M: f64 = 100.0;

/// The last stretch before a place is not held against the ground,
/// metres: there the line comes down to the place by design, and a
/// waypoint's height and the terrain's differ by a few metres.
const REACH_FINAL_M: f64 = 500.0;

/// Places further than this are not checked against the ground, metres:
/// the terrain held reaches some thirteen kilometres from the glider.
const REACH_GROUND_AT_MOST_M: f64 = 30_000.0;

fn reach_of(
    place: &TargetPlace,
    position: GeoPoint,
    state: &live::LiveState,
    glider: &Performance,
    terrain: Option<&Terrain>,
) -> ReachView {
    let unknown = ReachView {
        distance_m: 0.0,
        bearing_deg: 0.0,
        arrival_height_m: None,
        reach: Reach::Unknown,
        ground_in_the_way_m: None,
    };
    let Ok(there) = GeoPoint::new(place.position.lat, place.position.lon) else {
        return unknown;
    };
    let way = navigation::navigate(
        position,
        state.altitude_m,
        state.wind,
        &Target {
            name: String::new(),
            position: there,
            elevation_m: place.elevation_m.filter(|elevation| elevation.is_finite()),
        },
        glider,
    );
    let ground_in_the_way_m =
        ground_in_the_way(terrain, position, state.altitude_m, &way, place.elevation_m);
    ReachView {
        distance_m: way.distance_m,
        bearing_deg: way.bearing_deg,
        arrival_height_m: way.arrival_height_m,
        reach: if ground_in_the_way_m.is_some() {
            Reach::Beyond
        } else {
            reach_in_words(way.height_above_m, way.arrival_height_m)
        },
        ground_in_the_way_m,
    }
}

/// Where the straight line from the glider to where it is expected to be
/// over a place meets the ground before it gets there, metres from the
/// glider: a ridge in the way. `None` when the line is clear as far as the
/// ground is known, when the glide does not get there anyway, and without
/// terrain.
fn ground_in_the_way(
    terrain: Option<&Terrain>,
    position: GeoPoint,
    altitude_m: Option<f64>,
    way: &navigation::Navigation,
    elevation_m: Option<f64>,
) -> Option<f64> {
    let (terrain, altitude) = terrain.zip(altitude_m)?;
    let arrival = way.arrival_height_m.filter(|arrival| *arrival >= 0.0)?;
    let elevation = elevation_m.filter(|elevation| elevation.is_finite())?;
    if way.distance_m > REACH_GROUND_AT_MOST_M {
        return None;
    }
    let places = ((way.distance_m - REACH_FINAL_M) / REACH_GROUND_STEP_M).floor();
    if places < 1.0 {
        return None;
    }
    let fall = altitude - (elevation + arrival);
    terrain
        .profile(
            position,
            way.bearing_deg,
            REACH_GROUND_STEP_M,
            places as usize + 1,
        )
        .into_iter()
        .enumerate()
        // The glider's own place says nothing: on a slope its height
        // over the ground is the pilot's business.
        .skip(1)
        .find_map(|(index, ground)| {
            let along = index as f64 * REACH_GROUND_STEP_M;
            let line = altitude - fall * along / way.distance_m;
            ground.filter(|ground| line < *ground).map(|_| along)
        })
}

/// Whether a place is within reach, from how far the glider is above it
/// now and how far above it the glide is expected to end.
fn reach_in_words(height_above_m: Option<f64>, arrival_height_m: Option<f64>) -> Reach {
    match (height_above_m, arrival_height_m) {
        (None, _) => Reach::Unknown,
        // The heights are known and no arrival can be worked out: the wind
        // lets the glider make no way towards the place.
        (Some(_), None) => Reach::Beyond,
        (Some(_), Some(arrival)) if arrival >= REACH_RESERVE_M => Reach::Within,
        (Some(_), Some(arrival)) if arrival >= 0.0 => Reach::Just,
        (Some(_), Some(_)) => Reach::Beyond,
    }
}

/// A wind older than this, seconds, is not warned from: it was measured
/// somewhere else by now.
const STRONG_WIND_FRESH_S: f64 = 20.0 * 60.0;

/// The wind is called strong while the glider's trim speed leaves less
/// than this much way into it, metres per second: 10 km/h.
const STRONG_WIND_MARGIN_MS: f64 = 10.0 / 3.6;

/// What to say about a wind that leaves the glider little way into it, if
/// anything. The trim speed is the pilot's setting; with the speed bar a
/// glider makes more, which is what the words leave room for.
fn strong_wind_in_words(trim_speed_ms: f64, wind_speed_ms: f64, age_s: f64) -> Option<String> {
    let into_wind_ms = trim_speed_ms - wind_speed_ms;
    if !(0.0..=STRONG_WIND_FRESH_S).contains(&age_s) || into_wind_ms >= STRONG_WIND_MARGIN_MS {
        return None;
    }
    let into_wind_kmh = (into_wind_ms * 3.6).round();
    Some(if into_wind_kmh < 1.0 {
        "Wind as strong as the glider is fast".to_owned()
    } else {
        format!("Into the wind {into_wind_kmh:.0} km/h")
    })
}

/// A place whose climb is this far above the mean of the thermal's
/// picture, metres per second, is drawn at full size, and one as far
/// below at the smallest. A fixed measure, so that the dots of an even
/// thermal are all alike and do not show the vario's noise.
const FULL_STRENGTH_MS: f64 = 1.0;

/// While the lift lies less than this share of the circle's radius from the
/// middle of the circle, the circle counts as centred.
const CENTRED_SHARE: f64 = 0.2;

fn lift_view(picture: &live::ThermalPicture) -> LiftView {
    let mean = picture
        .points
        .iter()
        .map(|point| point.climb_ms)
        .sum::<f64>()
        / picture.points.len().max(1) as f64;
    LiftView {
        points: picture
            .points
            .iter()
            .map(|point| LiftPointView {
                lat: point.position.lat(),
                lon: point.position.lon(),
                climb_ms: point.climb_ms,
                strength: (0.5 + (point.climb_ms - mean) / (2.0 * FULL_STRENGTH_MS))
                    .clamp(0.0, 1.0),
            })
            .collect(),
        centre: picture.centre.map(|centre| LiftCentreView {
            position: Coordinate {
                lat: centre.position.lat(),
                lon: centre.position.lon(),
            },
            outline: (0..=48)
                .map(|step| {
                    let point = Earth::FaiSphere.destination(
                        centre.position,
                        f64::from(step) * 7.5,
                        centre.radius_m,
                    );
                    Coordinate {
                        lat: point.lat(),
                        lon: point.lon(),
                    }
                })
                .collect(),
            radius_m: centre.radius_m,
            shift_m: centre.shift_m,
            toward_deg: centre.toward_deg,
            stronger_by_ms: centre.stronger_by_ms,
            is_centred: centre.shift_m < CENTRED_SHARE * centre.radius_m,
            words: if centre.shift_m < CENTRED_SHARE * centre.radius_m {
                "Centred".to_owned()
            } else {
                format!(
                    "Stronger {:.0} m {}",
                    (centre.shift_m / 5.0).round() * 5.0,
                    compass(centre.toward_deg)
                )
            },
        }),
        drifted: picture.drifted,
    }
}

#[cfg(test)]
mod tests {
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    fn folder_text(folder: &TempFolder) -> String {
        folder.path().to_str().unwrap().to_owned()
    }

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    /// Plays the first `seconds` of the sample flight into a new session.
    fn fly(folder: &TempFolder, seconds: f64, record: bool) -> (Arc<FlightSession>, Arc<Replay>) {
        let session = FlightSession::new(folder_text(folder));
        if record {
            session.start_recording(None, None).unwrap();
        }
        let replay = Replay::new(sample()).unwrap();
        replay.advance(session.clone(), seconds);
        (session, replay)
    }

    #[test]
    fn the_serial_services_are_handed_to_the_shells() {
        let services = vario_serial_services();
        assert_eq!(services.len(), 3);
        assert!(
            services
                .iter()
                .any(|service| service.service.starts_with("6E400001"))
        );
    }

    #[test]
    fn a_shared_flight_sends_its_positions_from_take_off_and_a_replay_sends_none() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        let random: Vec<u8> = (0..26).collect();
        let view = session
            .share_live(
                "https://venti.example".to_owned(),
                random.clone(),
                LiveAudience::Public,
            )
            .unwrap();
        assert_eq!(view.words, "Begins at take-off");
        assert!(view.link.starts_with("https://venti.example/live/"));

        // Standing at the take-off for a minute: nothing to send.
        let flight = synthetic_flight();
        let start = flight.fixes[0].time.timestamp() as f64 - 60.0;
        let feed = |time: f64, fix: &Fix| {
            session.on_location(LocationSample {
                time_unix_s: time,
                lat: fix.position.lat(),
                lon: fix.position.lon(),
                altitude_m: Some(f64::from(fix.gps_altitude_m)),
                ground_speed_ms: None,
                course_deg: None,
            });
        };
        for second in 0..60 {
            feed(start + f64::from(second), &flight.fixes[0]);
            assert_eq!(session.live_request(start + f64::from(second)), None);
        }

        // In the air the first position goes out as soon as the take-off
        // is noticed, and the server's answer is taken.
        let mut first = None;
        for (index, fix) in flight.fixes.iter().enumerate().take(30) {
            let now = start + 60.0 + index as f64;
            feed(now, fix);
            if let Some(request) = session.live_request(now) {
                first = Some((index, request));
                break;
            }
        }
        let (index, request) = first.expect("a request within half a minute of flying");
        assert!(index <= 10, "{index}");
        assert_eq!(request.path, "/v1/live/positions");
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(body["visibility"], "public");
        assert_eq!(body["landed"], false);
        assert_eq!(body["fixes"].as_array().unwrap().len(), 1);
        let now = start + 60.0 + index as f64;
        session.live_answer(201, b"{}".to_vec(), None, now);
        let view = session.live_share(now + 5.0).unwrap();
        assert_eq!(
            (view.taken, view.state),
            (1, crate::LiveShareState::Sharing)
        );
        assert_eq!(view.words, "Sent 5 s ago");

        // The pilot ends the flight: said to the server at once.
        session.end_live();
        let last = session.live_request(now + 6.0).unwrap();
        let body: serde_json::Value = serde_json::from_slice(&last.body.unwrap()).unwrap();
        assert_eq!(body["landed"], true);

        // A replay is not shared, and sharing that was on is dropped when
        // a replay begins.
        let (replayed, _) = fly(&folder, 300.0, false);
        assert!(
            replayed
                .share_live(
                    "https://venti.example".to_owned(),
                    random.clone(),
                    LiveAudience::Link
                )
                .is_err()
        );
        assert_eq!(replayed.live_share(0.0), None);
        let session = FlightSession::new(folder_text(&folder));
        session
            .share_live(
                "https://venti.example".to_owned(),
                random,
                LiveAudience::Link,
            )
            .unwrap();
        Replay::new(sample())
            .unwrap()
            .advance(session.clone(), 300.0);
        assert_eq!(session.live_share(0.0), None);
        assert_eq!(session.live_request(1e10), None);
    }

    #[test]
    fn the_landings_within_reach_are_told_from_the_ones_beyond() {
        let folder = TempFolder::new();
        // Nothing to say before there is a position.
        let nowhere = FlightSession::new(folder_text(&folder));
        assert!(nowhere.reach(Vec::new(), 0.0).is_empty());
        assert_eq!(nowhere.landing_option(Vec::new(), 0.0), None);

        // Five minutes in: above the first thermal, west wind 3 m/s, the
        // default glider (38 km/h, a glide of 8).
        let (session, replay) = fly(&folder, 300.0, false);
        let view = session.view(replay.now());
        let (here, altitude) = (view.position.unwrap(), view.altitude_m.unwrap());
        let place = |name: &str, east_km: f64, below_m: Option<f64>| TargetPlace {
            name: name.to_owned(),
            position: Coordinate {
                lat: here.lat,
                lon: here.lon + east_km / 75.0,
            },
            elevation_m: below_m.map(|below| altitude - below),
        };
        let places = vec![
            // 4 km downwind and 900 m below: about 510 m to spare.
            place("Downwind", 4.0, Some(900.0)),
            // The same distance into the wind: a glide of 5.7 over the
            // ground does not make it.
            place("Upwind", -4.0, Some(600.0)),
            // 6 km downwind and 660 m below: arrives, with little to spare.
            place("Far", 6.0, Some(660.0)),
            place("No height", 1.0, None),
        ];
        let reach = session.reach(places.clone(), replay.now());
        assert_eq!(reach.len(), 4);
        assert_eq!(reach[0].reach, Reach::Within);
        assert!(
            (reach[0].arrival_height_m.unwrap() - 510.0).abs() < 40.0,
            "{:?}",
            reach[0]
        );
        assert!((reach[0].distance_m - 4000.0).abs() < 100.0);
        assert!((reach[0].bearing_deg - 90.0).abs() < 1.0);
        assert_eq!(reach[1].reach, Reach::Beyond);
        assert!(reach[1].arrival_height_m.unwrap() < 0.0, "{:?}", reach[1]);
        assert_eq!(reach[2].reach, Reach::Just);
        let spare = reach[2].arrival_height_m.unwrap();
        assert!((0.0..100.0).contains(&spare), "{spare}");
        assert_eq!(reach[3].reach, Reach::Unknown);
        assert_eq!(reach[3].arrival_height_m, None);

        // The landing to think of first is the one with most to spare, and
        // two of the four can be reached.
        let best = session
            .landing_option(places.clone(), replay.now())
            .unwrap();
        assert_eq!((best.index, best.name.as_str()), (0, "Downwind"));
        assert_eq!(best.in_reach, 2);
        assert_eq!(best.reach, reach[0]);

        // With only the one upwind there is no landing in reach, and it is
        // said by how much it is missed.
        let none = session
            .landing_option(vec![places[1].clone(), places[3].clone()], replay.now())
            .unwrap();
        assert_eq!((none.index, none.in_reach), (0, 0));
        assert_eq!(none.reach.reach, Reach::Beyond);
        // Places without a height say nothing at all.
        assert_eq!(
            session.landing_option(vec![places[3].clone()], replay.now()),
            None
        );

        // A wind the glider makes no way into gives no arrival to work
        // out, whatever the height: beyond reach, not unknown.
        assert_eq!(reach_in_words(Some(900.0), None), Reach::Beyond);
        assert_eq!(reach_in_words(None, None), Reach::Unknown);
        assert_eq!(reach_in_words(Some(900.0), Some(100.0)), Reach::Within);
        assert_eq!(reach_in_words(Some(900.0), Some(99.0)), Reach::Just);
        assert_eq!(reach_in_words(Some(900.0), Some(-1.0)), Reach::Beyond);
    }

    #[test]
    fn a_wind_that_leaves_little_way_into_it_is_said() {
        // A glider that trims at 38 km/h.
        let trim = 38.0 / 3.6;
        assert_eq!(strong_wind_in_words(trim, 3.0, 10.0), None);
        // 27 km/h of wind leave 11: not said. 30 leave 8: said.
        assert_eq!(strong_wind_in_words(trim, 27.0 / 3.6, 10.0), None);
        assert_eq!(
            strong_wind_in_words(trim, 30.0 / 3.6, 10.0).as_deref(),
            Some("Into the wind 8 km/h")
        );
        // As strong as the glider is fast, or stronger.
        for wind_kmh in [38.0, 45.0] {
            assert_eq!(
                strong_wind_in_words(trim, wind_kmh / 3.6, 10.0).as_deref(),
                Some("Wind as strong as the glider is fast")
            );
        }
        // Measured half an hour ago, somewhere else: not warned from.
        assert_eq!(strong_wind_in_words(trim, 45.0 / 3.6, 1800.0), None);
    }

    #[test]
    fn the_thermals_picture_says_where_the_lift_is_stronger() {
        let at = |east: f64, north: f64| {
            GeoPoint::new(47.5 + north / 111_195.0, 11.0 + east / 75_123.0).unwrap()
        };
        let place = |east: f64, north: f64, climb_ms: f64| live::LiftPoint {
            position: at(east, north),
            climb_ms,
            age_s: 0.0,
        };
        let centre = |shift_m: f64, toward_deg: f64| live::LiftCentre {
            position: at(0.0, 0.0),
            circle: at(0.0, 0.0),
            radius_m: 40.0,
            shift_m,
            toward_deg,
            climb_ms: 2.0,
            stronger_by_ms: shift_m / 40.0,
        };

        let view = lift_view(&live::ThermalPicture {
            points: vec![
                place(40.0, 0.0, 0.5),
                place(0.0, 40.0, 1.5),
                place(-40.0, 0.0, 2.0),
                place(0.0, -40.0, 4.0),
            ],
            centre: Some(centre(22.0, 300.0)),
            drifted: true,
        });
        // Around a mean of 2 m/s: a metre and a half below it is as small
        // as a dot gets, half a metre below is a quarter, the mean a half,
        // and two metres above as large as it gets.
        let strengths: Vec<f64> = view.points.iter().map(|point| point.strength).collect();
        assert_eq!(strengths, [0.0, 0.25, 0.5, 1.0]);
        let shown = view.centre.unwrap();
        assert_eq!(shown.words, "Stronger 20 m NW");
        assert!(!shown.is_centred);
        // The circle to fly has the radius of the circles flown.
        for corner in &shown.outline {
            let corner = GeoPoint::new(corner.lat, corner.lon).unwrap();
            assert!((corner.distance_m(at(0.0, 0.0)) - 40.0).abs() < 0.1);
        }

        // Lift that differs by the vario's noise gives dots nearly alike,
        // and a lean of a few metres is a centred circle.
        let view = lift_view(&live::ThermalPicture {
            points: vec![place(40.0, 0.0, 2.0), place(0.0, 40.0, 2.2)],
            centre: Some(centre(7.0, 90.0)),
            drifted: false,
        });
        assert!(
            view.points
                .iter()
                .all(|point| (point.strength - 0.5).abs() < 0.06)
        );
        let shown = view.centre.unwrap();
        assert!(shown.is_centred);
        assert_eq!(shown.words, "Centred");
        assert!(!view.drifted);
    }

    #[test]
    fn a_new_session_shows_nothing_and_is_on_the_ground() {
        let folder = TempFolder::new();
        let view = FlightSession::new(folder_text(&folder)).view(1_000.0);

        assert_eq!(view.phase, FlightPhase::Ground);
        assert!(!view.recording);
        assert_eq!(view.position, None);
        assert_eq!(view.climb_ms, None);
        assert_eq!(view.tone, None);
        assert_eq!(view.instrument, None);
        assert!(view.trail.is_empty());
    }

    #[test]
    fn the_glide_range_reaches_far_over_the_valley_and_stops_at_the_mountain() {
        let folder = TempFolder::new();
        // Nine minutes in: 1,900 m over the valley, 1,150 m over the
        // ground, with the Wallberg to the south-east and the lake to the
        // north-west.
        let (session, replay) = fly(&folder, 540.0, false);
        assert_eq!(session.glide_range(replay.now()), None);

        let terrain = Terrain::new();
        terrain
            .add_archive(
                std::fs::read(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../apps/ios/Venti/Resources/MapDemo/terrain.pmtiles"
                ))
                .unwrap(),
            )
            .unwrap();
        session.use_terrain(Some(terrain));
        let range = session.glide_range(replay.now()).unwrap();

        // A ring of 72 places and the first again.
        assert_eq!(range.outline.len(), 73);
        assert_eq!(range.outline[0], range.outline[72]);
        assert_eq!(range.reserve_m, 100.0);

        let here = session.view(replay.now()).position.unwrap();
        let here = GeoPoint::new(here.lat, here.lon).unwrap();
        let reach = |bearing: usize| {
            let place = range.outline[bearing / 5];
            here.distance_m(GeoPoint::new(place.lat, place.lon).unwrap())
        };
        // Out over the lake to the north-north-west the glide is long: a
        // thousand metres to lose at a glide of seven. Downwind along the
        // valley to the east-north-east it is longer still. Towards the
        // mountain in the south-east it ends on the slope, two kilometres
        // out, and that is where the edge is nearest.
        assert!(reach(330) > 6000.0, "{}", reach(330));
        assert!(reach(75) > 8000.0, "{}", reach(75));
        assert!(reach(135) < 3000.0, "{}", reach(135));
        assert!((range.nearest_m - reach(135)).abs() < 400.0);
        assert!(range.farthest_m > 8000.0 && range.farthest_m < 12_000.0);
        // All of it over ground the app holds.
        assert!(!range.is_cut_short);

        // On the ground there is no range to draw.
        let (session, replay) = fly(&TempFolder::new(), 0.0, false);
        assert_eq!(session.glide_range(replay.now()), None);
    }

    #[test]
    fn a_ridge_in_the_way_puts_a_landing_out_of_reach() {
        let folder = TempFolder::new();
        // Five minutes in: over the Wallberg at 2,016 m, 570 m over its
        // north side.
        let (session, replay) = fly(&folder, 300.0, false);
        let place = |name: &str, lat: f64, lon: f64, elevation_m: f64| TargetPlace {
            name: name.to_owned(),
            position: Coordinate { lat, lon },
            elevation_m: Some(elevation_m),
        };
        // A field in the valley to the north, down the open slope, and a
        // meadow in the valley south of the mountain, behind its ridge.
        let landings = vec![
            place("Behind the ridge", 47.63, 11.83, 1176.0),
            place("Down the slope", 47.6833, 11.755, 744.0),
        ];

        // Without terrain the straight glide reaches both, the far one
        // with 260 m to spare.
        let reaches = session.reach(landings.clone(), replay.now());
        assert_eq!(reaches[0].reach, Reach::Within);
        assert!((reaches[0].arrival_height_m.unwrap() - 259.0).abs() < 30.0);
        assert_eq!(reaches[0].ground_in_the_way_m, None);
        assert_eq!(reaches[1].reach, Reach::Within);

        // With the ground the app has for the region, the line to the
        // meadow meets the ridge four kilometres out. The arrival height
        // still says what a straight glide would give; the place is out
        // of reach.
        let terrain = Terrain::new();
        terrain
            .add_archive(
                std::fs::read(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../apps/ios/Venti/Resources/MapDemo/terrain.pmtiles"
                ))
                .unwrap(),
            )
            .unwrap();
        session.use_terrain(Some(terrain));
        let reaches = session.reach(landings.clone(), replay.now());
        assert_eq!(reaches[0].reach, Reach::Beyond);
        let meets = reaches[0].ground_in_the_way_m.unwrap();
        assert!((3500.0..4500.0).contains(&meets), "{meets}");
        assert!(reaches[0].arrival_height_m.unwrap() > 200.0);
        assert_eq!(reaches[1].reach, Reach::Within);
        assert_eq!(reaches[1].ground_in_the_way_m, None);

        // The landing to think of first is one with nothing in the way,
        // and only one of the two counts as within reach.
        let option = session.landing_option(landings, replay.now()).unwrap();
        assert_eq!(option.name, "Down the slope");
        assert_eq!(option.in_reach, 1);

        // With only the blocked one to choose from, that is the one
        // named, as beyond reach.
        let only = vec![place("Behind the ridge", 47.63, 11.83, 1176.0)];
        let option = session.landing_option(only, replay.now()).unwrap();
        assert_eq!(option.reach.reach, Reach::Beyond);
        assert!(option.reach.ground_in_the_way_m.is_some());
        assert_eq!(option.in_reach, 0);

        // Chosen as the place to fly to, the way to it says the same;
        // the way to the field down the slope is clear.
        session.go_to(Some(place("Behind the ridge", 47.63, 11.83, 1176.0)));
        let way = session.navigation(replay.now()).unwrap();
        assert!(way.arrival_height_m.unwrap() > 200.0);
        assert_eq!(way.ground_in_the_way_m, Some(meets));
        session.go_to(Some(place("Down the slope", 47.6833, 11.755, 744.0)));
        let way = session.navigation(replay.now()).unwrap();
        assert_eq!(way.ground_in_the_way_m, None);
    }

    #[test]
    fn a_replayed_flight_shows_what_the_pilot_would_have_seen() {
        let folder = TempFolder::new();
        // Five minutes in: deep in the first thermal, climbing 2.4 m/s.
        let (session, replay) = fly(&folder, 300.0, false);
        let view = session.view(replay.now());

        assert_eq!(view.phase, FlightPhase::Flying);
        assert!(
            (view.flight_time_s.unwrap() - 300).abs() <= 3,
            "{:?}",
            view.flight_time_s
        );
        assert!(view.circling);
        assert!((view.climb_ms.unwrap() - 2.4).abs() < 0.3);
        assert_eq!(view.vario_source, Some(VarioSourceKind::Pressure));
        assert!(view.thermal.unwrap().gain_m > 350.0);
        let wind = view.wind.unwrap();
        assert!((wind.from_deg - 270.0).abs() < 8.0 && (wind.speed_ms - 3.0).abs() < 0.5);
        // Measured this very moment, give or take the two seconds the fit
        // runs behind; and 11 km/h is no strong wind.
        assert!(view.wind_age_s.unwrap() < 5.0, "{:?}", view.wind_age_s);
        assert_eq!(view.strong_wind, None);
        assert_eq!(view.trail.len(), 181);
        assert!(view.position_age_s.unwrap() < 1.0);
        // Climbing in the air: the vario beeps.
        let tone = view.tone.unwrap();
        assert!(tone.frequency_hz > 700.0 && tone.duty < 1.0);
        // A thermal is marked once it is left, not while the glider is in it.
        assert!(view.thermals.is_empty());
        // Its picture: the last fifty seconds as the circles flown, with a
        // circle to fly drawn around where the lift is. The made-up thermal
        // lifts evenly, so that is where the glider circles already.
        let lift = view.lift.as_ref().unwrap();
        assert!(lift.drifted);
        assert_eq!(lift.points.len(), 51);
        assert!(
            lift.points
                .iter()
                .all(|point| (0.0..=1.0).contains(&point.strength))
        );
        let centre = lift.centre.as_ref().unwrap();
        assert!((centre.radius_m - 45.0).abs() < 3.0, "{centre:?}");
        assert_eq!(centre.words, "Centred");
        assert_eq!(centre.outline.len(), 49);
        assert_eq!(centre.outline.first(), centre.outline.last());

        // Ten minutes in, on the glide after it: the thermal is on the map
        // with its climb, to be found again.
        let folder = TempFolder::new();
        let (session, replay) = fly(&folder, 600.0, false);
        let view = session.view(replay.now());
        assert!(!view.circling);
        assert_eq!(view.lift, None);
        assert_eq!(view.thermals.len(), 1);
        // The wind is from the thermal left more than four minutes ago.
        let age = view.wind_age_s.unwrap();
        assert!((240.0..=275.0).contains(&age), "{age}");
        // The wind the circles gave still stands on the glide. A flight
        // file's positions are rounded to two metres, which makes every
        // second of straight flight look like a small change of velocity:
        // none of that may be taken for circling and fitted.
        for seconds in [345.0, 350.0, 360.0, 380.0, 450.0, 600.0] {
            let folder = TempFolder::new();
            let (session, replay) = fly(&folder, seconds, false);
            let wind = session.view(replay.now()).wind.unwrap();
            assert!(
                (wind.from_deg - 270.0).abs() < 5.0 && (wind.speed_ms - 3.0).abs() < 0.25,
                "{seconds} s: {wind:?}"
            );
        }
        let mark = &view.thermals[0];
        assert!(mark.gain_m > 500.0, "{mark:?}");
        assert!(mark.label.starts_with("+2."), "{mark:?}");
        assert!(mark.top_altitude_m > mark.gain_m);

        // The wind of every band of height the climb went through, the
        // highest first; the glider has since sunk out of the top band.
        assert!(view.winds_by_height.len() >= 3);
        let top = view.winds_by_height[0];
        assert_eq!(top.ceiling_m - top.floor_m, 200.0);
        assert!(
            (top.from_deg - 270.0).abs() < 12.0 && (top.speed_ms - 3.0).abs() < 0.8,
            "{:?}",
            view.winds_by_height
        );
        assert!(top.age_s > 60.0, "{top:?}");
        // Each band also says how the thermal climbed in it.
        let climbs: Vec<f64> = view
            .winds_by_height
            .iter()
            .filter_map(|band| band.climb_ms)
            .collect();
        assert!(climbs.len() >= 2, "{:?}", view.winds_by_height);
        assert!(
            climbs.iter().all(|climb| (climb - 2.4).abs() < 0.3),
            "{climbs:?}"
        );
        assert!(
            view.winds_by_height
                .iter()
                .filter(|band| band.is_here)
                .count()
                <= 1,
            "{:?}",
            view.winds_by_height
        );
    }

    #[test]
    fn with_a_task_the_cockpit_navigates_to_its_next_turnpoint() {
        let folder = TempFolder::new();
        let (session, replay) = fly(&folder, 300.0, false);
        let now = replay.now();
        assert_eq!(session.task_status(now), None);
        let here = session.view(now).position.unwrap();

        // A task from here: a turnpoint about 6 km east, the goal 12 km.
        let east = |km: f64| here.lon + km / 75.0;
        let task = format!(
            r#"{{"taskType":"CLASSIC","version":1,"turnpoints":[
                {{"type":"TAKEOFF","radius":400,"waypoint":{{"name":"HERE","lat":{lat},"lon":{lon},"altSmoothed":900}}}},
                {{"radius":1000,"waypoint":{{"name":"RIDGE","lat":{lat},"lon":{ridge},"altSmoothed":1500}}}},
                {{"radius":400,"waypoint":{{"name":"FIELD","lat":{lat},"lon":{field},"altSmoothed":700}}}}]}}"#,
            lat = here.lat,
            lon = here.lon,
            ridge = east(6.0),
            field = east(12.0),
        );
        session.fly_task(Some(task), now).unwrap();
        // Nothing to show until the next position comes in.
        assert_eq!(session.task_status(now), None);
        session.on_location(LocationSample {
            time_unix_s: now + 1.0,
            lat: here.lat,
            lon: here.lon,
            altitude_m: session.view(now).altitude_m,
            ground_speed_ms: None,
            course_deg: None,
        });

        let status = session.task_status(now + 1.0).unwrap();
        assert_eq!(status.phase, crate::TaskPhase::OnTask);
        let next = status.next.unwrap();
        assert_eq!((next.number, next.name.as_str()), (2, "RIDGE"));
        // To the edge of the ridge's cylinder, and on to the goal's.
        assert!(
            (next.distance_m - 5_000.0).abs() < 150.0,
            "{}",
            next.distance_m
        );
        assert!(
            (status.to_goal_m - 11_600.0).abs() < 300.0,
            "{}",
            status.to_goal_m
        );
        assert!(status.gate.is_none());
        assert!(status.above_goal_m.is_some());

        // The cockpit's navigation follows the task.
        let way = session.navigation(now + 1.0).unwrap();
        assert!(!way.is_takeoff);
        assert_eq!(way.name, "RIDGE");
        // The same point; the task measures on the ellipsoid and the glide
        // on the sphere, which is a few metres in five kilometres.
        assert!((way.distance_m - next.distance_m).abs() < 0.005 * next.distance_m);
        assert!((way.bearing_deg - 90.0).abs() < 2.0);

        // A place the pilot chooses comes first; letting go of it goes
        // back to the task, and stepping on moves to the goal.
        session.go_to(Some(TargetPlace {
            name: "Landeplatz".to_owned(),
            position: here,
            elevation_m: None,
        }));
        assert_eq!(session.navigation(now + 1.0).unwrap().name, "Landeplatz");
        session.go_to(None);
        assert_eq!(session.navigation(now + 1.0).unwrap().name, "RIDGE");
        session.step_task(true);
        assert_eq!(session.navigation(now + 1.0).unwrap().name, "FIELD");
        assert!(session.take_task_event().is_none());

        // Without a task it is the take-off again.
        session.fly_task(None, now).unwrap();
        assert!(session.navigation(now + 1.0).unwrap().is_takeoff);
        // What is not a task is refused and changes nothing.
        assert!(session.fly_task(Some("nothing".to_owned()), now).is_err());
    }

    #[test]
    fn the_cockpit_navigates_back_to_the_take_off_until_a_place_is_chosen() {
        let folder = TempFolder::new();
        // Nothing to navigate to before there is a position or a take-off.
        assert_eq!(
            FlightSession::new(folder_text(&folder)).navigation(1_000.0),
            None
        );

        // Five minutes in: circling above the first thermal, west wind 3 m/s.
        let (session, replay) = fly(&folder, 300.0, false);
        let view = session.view(replay.now());
        let home = session.navigation(replay.now()).unwrap();
        assert!(home.is_takeoff);
        assert_eq!(home.name, "Take-off");
        assert_eq!(Some(home.position), view.takeoff);
        assert_eq!(Some(home.distance_m), view.distance_to_takeoff_m);
        assert_eq!(Some(home.bearing_deg), view.bearing_to_takeoff_deg);
        // The take-off's height is the height the glider left the ground at.
        assert_eq!(home.height_above_m, view.height_above_takeoff_m);
        assert!(home.wind_known);
        assert!(home.arrival_height_m.is_some());

        // A landing field 4 km east and 900 m below: downwind, well in reach.
        let here = view.position.unwrap();
        let altitude = view.altitude_m.unwrap();
        session.go_to(Some(TargetPlace {
            name: "Landeplatz".to_owned(),
            position: Coordinate {
                lat: here.lat,
                lon: here.lon + 4.0 / 75.0,
            },
            elevation_m: Some(altitude - 900.0),
        }));
        let way = session.navigation(replay.now()).unwrap();
        assert!(!way.is_takeoff);
        assert_eq!(way.name, "Landeplatz");
        assert!(
            (way.distance_m - 4000.0).abs() < 100.0,
            "{}",
            way.distance_m
        );
        assert!((way.bearing_deg - 90.0).abs() < 1.0);
        assert!((way.required_glide.unwrap() - 4.44).abs() < 0.15);
        // The default glider: 38 km/h and a glide of 8, with 3 m/s from
        // behind: about 10 over the ground.
        assert!(
            (way.expected_glide.unwrap() - 10.3).abs() < 0.5,
            "{:?}",
            way.expected_glide
        );
        assert!(
            (way.arrival_height_m.unwrap() - 510.0).abs() < 40.0,
            "{:?}",
            way.arrival_height_m
        );

        // A slower glider with a worse glide arrives lower; nonsense is ignored.
        session.set_glider(9.0, 6.0);
        let slower = session.navigation(replay.now()).unwrap();
        assert!(slower.arrival_height_m.unwrap() < way.arrival_height_m.unwrap() - 80.0);
        session.set_glider(0.0, 100.0);
        assert_eq!(session.navigation(replay.now()), Some(slower));

        // Choosing nothing goes back to the take-off.
        session.go_to(None);
        assert!(session.navigation(replay.now()).unwrap().is_takeoff);
    }

    #[test]
    fn the_cockpit_knows_how_much_of_the_day_is_left() {
        let folder = TempFolder::new();
        assert_eq!(
            FlightSession::new(folder_text(&folder)).daylight(1_000.0),
            None
        );

        // The sample flight: 4 July 2026 above the Tegernsee, from half
        // past ten UTC. Five minutes in, sunset is more than eight hours
        // off, at about 19:13 UTC there, and night follows half an hour
        // and more later.
        let (session, replay) = fly(&folder, 300.0, false);
        let left = session.daylight(replay.now()).unwrap();
        let to_sunset = left.minutes_to_sunset.unwrap();
        assert!((510..=530).contains(&to_sunset), "{to_sunset}");
        let twilight = left.minutes_to_dusk.unwrap() - to_sunset;
        assert!((30..=45).contains(&twilight), "{twilight}");

        // Asked nine hours later from the same place, the sun has set.
        let later = session.daylight(replay.now() + 9.0 * 3600.0).unwrap();
        assert!(later.minutes_to_sunset.unwrap() < 0);
    }

    #[test]
    fn the_flight_is_scored_while_it_is_flown() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        // On the ground there is nothing to score.
        assert_eq!(session.live_score(), None);

        let replay = Replay::new(sample()).unwrap();
        replay.advance(session.clone(), 600.0);
        let early = session.live_score().unwrap();
        assert!(early.distance_m > 1000.0, "{}", early.distance_m);
        assert!(early.route.len() >= 2);
        // Asked again at once, the same answer comes back.
        assert_eq!(session.live_score(), Some(early.clone()));

        while replay.advance(session.clone(), 30.0) {}
        let whole = session.live_score().unwrap();
        assert!(whole.distance_m > early.distance_m);

        // The same flight in the logbook scores the same to within the
        // ten seconds between the positions kept in the air.
        let finished = scoring::score(venti_flight::analysis::flown_part(
            &igc::parse(&sample()).unwrap().fixes,
        ))
        .unwrap();
        assert!(
            (whole.distance_m - finished.distance_m).abs() < 400.0,
            "{} against {}",
            whole.distance_m,
            finished.distance_m
        );
    }

    #[test]
    fn an_open_triangle_shows_its_closing_circle_and_what_it_scores_closed() {
        // Round a triangle of some 12 km a side and back towards the
        // start, a fix every ten seconds as the cockpit keeps them, and
        // stop 4 km short of it.
        let start = DateTime::from_timestamp(1_783_150_000, 0).unwrap();
        let place = |east_km: f64, north_km: f64| {
            GeoPoint::new(
                47.0 + north_km / 111.195,
                11.0 + east_km / (111.195 * 47.0_f64.to_radians().cos()),
            )
            .unwrap()
        };
        let corners: [(f64, f64); 4] = [(0.0, 0.0), (12.0, 0.0), (7.0, 12.0), (2.0, 3.46)];
        let mut flown = Vec::new();
        for leg in corners.windows(2) {
            let ((e0, n0), (e1, n1)) = (leg[0], leg[1]);
            let steps = (e1 - e0).hypot(n1 - n0).mul_add(10.0, 1.0) as usize;
            for step in 0..steps {
                let part = step as f64 / steps as f64;
                flown.push(Fix {
                    time: start + chrono::Duration::seconds(10 * flown.len() as i64),
                    position: place(e0 + (e1 - e0) * part, n0 + (n1 - n0) * part),
                    gps_altitude_m: 2000,
                    pressure_altitude_m: 2000,
                    valid: true,
                });
            }
        }
        let scores = scoring::scores(&flown).unwrap();
        let view = live_score(&scores, &flown);
        assert!(!view.closed);
        let circle = view.closing.unwrap();
        // 5% of some 37 km around the start; the glider is 4 km from the
        // start, so a little over 2 km from the edge.
        assert!((circle.radius_m - 0.05 * circle.perimeter_m).abs() < 1e-6);
        let edge = circle.distance_to_edge_m.unwrap();
        let here = flown.last().unwrap().position;
        let centre = GeoPoint::new(circle.centre.lat, circle.centre.lon).unwrap();
        assert!((edge - (here.distance_m(centre) - circle.radius_m)).abs() < 1e-6);
        assert!(edge > 1_500.0 && edge < 3_500.0, "{edge}");
        assert!(
            circle.words.starts_with("Closing circle 2.")
                && circle.words.contains("closed FAI triangle of"),
            "{}",
            circle.words
        );
        assert!(circle.closed_points > view.points);

        // A closed triangle has nothing left to close.
        flown.push(Fix {
            time: flown.last().unwrap().time + chrono::Duration::seconds(600),
            position: place(0.1, 0.1),
            gps_altitude_m: 2000,
            pressure_altitude_m: 2000,
            valid: true,
        });
        let closed = live_score(&scoring::scores(&flown).unwrap(), &flown);
        assert!(closed.closed);
        assert_eq!(closed.closing, None);
    }

    #[test]
    fn the_areas_for_an_fai_triangle_stand_on_the_longest_line_flown() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        assert_eq!(session.fai_areas(), None);

        let replay = Replay::new(sample()).unwrap();
        // Two minutes in the glider has not gone three kilometres yet.
        replay.advance(session.clone(), 120.0);
        assert_eq!(session.fai_areas(), None);

        while replay.advance(session.clone(), 30.0) {}
        let areas = session.fai_areas().unwrap();
        let point = |place: &Coordinate| GeoPoint::new(place.lat, place.lon).unwrap();
        let (from, to) = (point(&areas.side[0]), point(&areas.side[1]));
        let side = from.distance_m(to);
        // The sample flight covers some six kilometres in a straight line.
        assert!(side > 3_000.0 && side < 12_000.0, "{side}");
        assert!((areas.smallest_m - side / 0.44).abs() < 1.0);
        assert!((areas.largest_m - side / 0.28).abs() < 1.0);
        assert_eq!(areas.outlines.len(), 2);
        for outline in &areas.outlines {
            assert_eq!(outline.first(), outline.last());
            // Every corner of an outline is a third corner that just makes
            // an FAI triangle of the side.
            for corner in outline {
                let (a, b) = (from.distance_m(point(corner)), to.distance_m(point(corner)));
                let share = side.min(a).min(b) / (side + a + b);
                assert!((share - 0.28).abs() < 0.003, "{share}");
            }
        }
        // The flight ends at one end of its longest line or near it, which
        // is no corner of a triangle.
        assert!(!areas.is_inside);
    }

    #[test]
    fn the_whole_flight_can_be_replayed_and_then_the_replay_is_over() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        let replay = Replay::new(sample()).unwrap();

        let mut steps = 0;
        while replay.advance(session.clone(), 7.5) {
            steps += 1;
        }
        assert!((230..=235).contains(&steps), "{steps}");
        assert!(!replay.advance(session.clone(), 60.0));
        let view = session.view(replay.now());
        assert_eq!(view.phase, FlightPhase::Flying);
        // The replay clock has run past the last fix: the fix is now old.
        assert!(view.position_age_s.unwrap() > 30.0);
    }

    #[test]
    fn a_recording_becomes_a_flight_file_with_every_fix() {
        let folder = TempFolder::new();
        let (session, _) = fly(&folder, 120.0, true);
        let view = session.view(0.0);
        assert!(view.recording);
        assert_eq!(view.recorded_fixes, 121);

        let recording = session.stop_recording().unwrap().unwrap();

        assert_eq!(recording.name, "20260704-103000.igc");
        let file = igc::parse(&recording.igc_text).unwrap();
        assert_eq!(file.fixes.len(), 121);
        assert_eq!(file.manufacturer, "XVE");
        assert_eq!(file.skipped_lines, 0);
        // The recorded fixes are the flight's own, to the file's precision.
        let original = synthetic_flight();
        assert_eq!(file.fixes[120].time, original.fixes[120].time);
        assert_eq!(
            file.fixes[120].gps_altitude_m,
            original.fixes[120].gps_altitude_m
        );
        assert!(
            file.fixes[120]
                .position
                .distance_m(original.fixes[120].position)
                < 2.0
        );
        assert!(file.has_pressure_altitude());
        assert!(!session.view(0.0).recording);
        assert_eq!(session.stop_recording().unwrap(), None);
    }

    #[test]
    fn a_recording_cut_short_is_found_at_the_next_launch_and_cleared_once_imported() {
        let folder = TempFolder::new();
        let (session, _) = fly(&folder, 45.0, true);
        // The app dies here: the session is dropped without stopping.
        drop(session);

        let pending = pending_recordings(folder_text(&folder));
        assert_eq!(pending.len(), 1);
        assert_eq!(igc::parse(&pending[0].igc_text).unwrap().fixes.len(), 46);

        clear_recording(folder_text(&folder), pending[0].name.clone()).unwrap();
        assert!(pending_recordings(folder_text(&folder)).is_empty());
        // Clearing twice, or something that is not a recording's name, is harmless.
        clear_recording(folder_text(&folder), pending[0].name.clone()).unwrap();
        clear_recording(folder_text(&folder), "../logbook-index.json".to_owned()).unwrap();
    }

    #[test]
    fn the_pilot_and_glider_are_written_into_the_recording_as_plain_text() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        session
            .start_recording(
                Some("  Jörg\nMüller ".to_owned()),
                Some("Advance Iota DLS 25".to_owned()),
            )
            .unwrap();
        Replay::new(sample())
            .unwrap()
            .advance(session.clone(), 10.0);

        let recording = session.stop_recording().unwrap().unwrap();
        let file = igc::parse(&recording.igc_text).unwrap();
        // The format is ASCII: letters it cannot hold are dropped, and a
        // line break cannot smuggle in a second header line.
        assert_eq!(file.pilot.as_deref(), Some("JrgMller"));
        assert_eq!(file.glider_type.as_deref(), Some("Advance Iota DLS 25"));
        assert_eq!(file.skipped_lines, 0);
        assert_eq!(header_text(Some(" \n ".to_owned())), None);
    }

    #[test]
    fn a_recording_without_a_fix_leaves_nothing_behind() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        session.start_recording(None, None).unwrap();
        assert_eq!(session.stop_recording().unwrap(), None);
        assert!(pending_recordings(folder_text(&folder)).is_empty());
    }

    /// A session at 47 N 11 E: a second standing, walking or flying
    /// straight north.
    fn step(session: &FlightSession, time_s: f64, north_m: f64, altitude_m: f64) {
        session.on_location(LocationSample {
            time_unix_s: time_s,
            lat: 47.0 + north_m / 111_194.9,
            lon: 11.0,
            altitude_m: Some(altitude_m),
            ground_speed_ms: None,
            course_deg: None,
        });
    }

    /// Five minutes at the take-off, three minutes of flight at 9 m/s
    /// sinking 1 m/s, then standing in the landing field until `until_s`
    /// seconds after the session began. Returns when the session began.
    fn stand_fly_and_land(session: &FlightSession, until_s: i32) -> f64 {
        let began = 1_786_000_000.0;
        for second in 0..until_s {
            let time = began + f64::from(second);
            let flown = f64::from((second - 300).clamp(0, 180));
            step(session, time, 9.0 * flown, 1600.0 - flown);
        }
        began
    }

    #[test]
    fn a_flight_records_itself_from_half_a_minute_before_its_take_off() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        let began = 1_786_000_000.0;
        session.record_from_takeoff(None, None, began).unwrap();

        // Five minutes at the take-off: waiting, and nothing written.
        stand_fly_and_land(&session, 300);
        let view = session.view(began + 300.0);
        assert_eq!(view.phase, FlightPhase::Ground);
        assert!(view.waiting_for_takeoff);
        assert!(!view.recording);
        assert_eq!(view.recorded_fixes, 0);
        assert_eq!(view.over, None);
        assert!(pending_recordings(folder_text(&folder)).is_empty());

        // In the air: recording, with the half minute before.
        stand_fly_and_land(&session, 330);
        let view = session.view(began + 330.0);
        assert_eq!(view.phase, FlightPhase::Flying);
        assert!(!view.waiting_for_takeoff);
        assert!(view.recording);
        assert_eq!(view.saves_in_s, None);

        // Landed: saved by itself two minutes after coming to rest.
        stand_fly_and_land(&session, 540);
        let view = session.view(began + 540.0);
        assert_eq!(view.phase, FlightPhase::Landed);
        assert_eq!(view.over, None);
        let left = view.saves_in_s.unwrap();
        assert!((55..=70).contains(&left), "{left}");
        let view = session.view(began + 540.0 + left as f64);
        assert_eq!(view.over, Some(SessionOver::Landed));
        assert_eq!(view.saves_in_s, Some(0));

        let recording = session.stop_recording().unwrap().unwrap();
        let file = igc::parse(&recording.igc_text).unwrap();
        let first = file.fixes[0].time.timestamp() as f64 - began;
        // The take-off is believed some seconds into the run, and the file
        // begins half a minute before that.
        assert!((270.0..=280.0).contains(&first), "{first}");
        assert_eq!(session.view(began + 700.0).over, None);
    }

    #[test]
    fn a_hop_is_thrown_away_and_the_session_waits_again() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        let began = 1_786_000_000.0;
        session.record_from_takeoff(None, None, began).unwrap();
        // A minute at the take-off, twelve seconds of running with the
        // wing up at 5 m/s, and standing again.
        let hop = |second: i32| {
            let run = f64::from((second - 60).clamp(0, 12));
            step(&session, began + f64::from(second), 5.0 * run, 1600.0);
        };
        (0..75).for_each(hop);
        let view = session.view(began + 75.0);
        assert_eq!(view.phase, FlightPhase::Flying);
        assert!(view.recording);

        (75..110).for_each(hop);
        let view = session.view(began + 110.0);
        assert_eq!(view.phase, FlightPhase::Ground);
        assert!(view.waiting_for_takeoff);
        assert!(!view.recording);
        assert_eq!(view.recorded_fixes, 0);
        assert_eq!(view.over, None);
        assert_eq!(view.flight_time_s, None);
        assert!(pending_recordings(folder_text(&folder)).is_empty());

        // The flight that follows is recorded and ends by itself, and
        // nothing of the hop is in its file.
        for second in 110..700 {
            let flown = f64::from((second - 300).clamp(0, 180));
            step(
                &session,
                began + f64::from(second),
                60.0 + 9.0 * flown,
                1600.0 - flown,
            );
        }
        assert_eq!(session.view(began + 700.0).over, Some(SessionOver::Landed));
        let recording = session.stop_recording().unwrap().unwrap();
        let file = igc::parse(&recording.igc_text).unwrap();
        let first = file.fixes[0].time.timestamp() as f64 - began;
        assert!((270.0..=280.0).contains(&first), "{first}");
    }

    #[test]
    fn a_session_that_waits_two_hours_for_a_take_off_is_over_with_nothing_to_save() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        let began = 1_786_000_000.0;
        session.record_from_takeoff(None, None, began).unwrap();
        for second in 0..60 {
            step(&session, began + f64::from(second), 0.0, 1600.0);
        }
        assert_eq!(session.view(began + 7199.0).over, None);
        assert_eq!(
            session.view(began + 7200.0).over,
            Some(SessionOver::NoTakeoff)
        );
        assert_eq!(session.stop_recording().unwrap(), None);
        assert!(pending_recordings(folder_text(&folder)).is_empty());
    }

    #[test]
    fn recording_from_now_ends_the_wait_and_keeps_the_half_minute() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        let began = 1_786_000_000.0;
        session.record_from_takeoff(None, None, began).unwrap();
        for second in 0..60 {
            step(&session, began + f64::from(second), 0.0, 1600.0);
        }
        session.start_recording(None, None).unwrap();
        let view = session.view(began + 60.0);
        assert!(view.recording);
        assert!(!view.waiting_for_takeoff);
        assert_eq!(view.recorded_fixes, 31);
        // Ground handling for hours is not given up on.
        assert_eq!(session.view(began + 8000.0).over, None);
        // A flight after it still ends by itself.
        for second in 60..600 {
            let flown = f64::from((second - 300).clamp(0, 180));
            step(
                &session,
                began + f64::from(second),
                9.0 * flown,
                1600.0 - flown,
            );
        }
        assert_eq!(session.view(began + 700.0).over, Some(SessionOver::Landed));
    }

    #[test]
    fn a_flight_the_pilot_starts_does_not_end_by_itself() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        session.start_recording(None, None).unwrap();
        let began = stand_fly_and_land(&session, 700);
        let view = session.view(began + 700.0);
        assert_eq!(view.phase, FlightPhase::Landed);
        assert_eq!(view.over, None);
        assert_eq!(view.saves_in_s, None);
        // Recorded from the tap: the standing before the flight is in it.
        assert_eq!(view.recorded_fixes, 700);
    }

    #[test]
    fn positions_within_one_second_are_recorded_once() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        session.start_recording(None, None).unwrap();
        for tenth in 0..25 {
            session.on_location(LocationSample {
                time_unix_s: 1_783_161_000.0 + f64::from(tenth) / 10.0,
                lat: 47.6,
                lon: 11.8,
                altitude_m: Some(1500.0),
                ground_speed_ms: Some(9.0),
                course_deg: Some(90.0),
            });
        }
        assert_eq!(session.view(0.0).recorded_fixes, 3);
    }

    #[test]
    fn the_course_is_written_once_the_glider_moves_enough_for_it_to_mean_something() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        let began = 1_783_161_000.0;
        let start = GeoPoint::new(47.6, 11.8).unwrap();
        let at = |second: u32, metres: f64, speed: Option<f64>, course: Option<f64>| {
            let here = venti_airspace::destination(start, 285.0, metres).unwrap();
            session.on_location(LocationSample {
                time_unix_s: began + f64::from(second),
                lat: here.lat(),
                lon: here.lon(),
                altitude_m: Some(1500.0),
                ground_speed_ms: speed,
                course_deg: course,
            });
            session.view(began + f64::from(second))
        };
        // One position gives no course, and no words.
        let mut view = at(0, 0.0, None, None);
        assert_eq!((view.course_deg, &view.course_words), (None, &None));

        // West-north-west at 10 m/s, worked out from the positions.
        for second in 1..=4 {
            view = at(second, 10.0 * f64::from(second), None, None);
        }
        assert!((view.course_deg.unwrap() - 285.0).abs() < 0.5);
        assert_eq!(view.course_words.as_deref(), Some("285° W"));

        // The receiver's own course is written the same way, in whole
        // degrees from 0 to 359.
        let view = at(5, 50.0, Some(9.0), Some(359.7));
        assert_eq!(view.course_words.as_deref(), Some("0° N"));
        let view = at(6, 60.0, Some(9.0), Some(44.6));
        assert_eq!(view.course_words.as_deref(), Some("45° NE"));

        // Slower than 2 m/s a course says nothing: the number is still
        // there for the arrow, the words are not.
        let view = at(7, 61.0, Some(1.5), Some(90.0));
        assert_eq!(view.course_deg, Some(90.0));
        assert_eq!(view.course_words, None);
        let view = at(8, 63.0, Some(2.0), Some(90.0));
        assert_eq!(view.course_words.as_deref(), Some("90° E"));

        // Once positions stop there is neither.
        let view = session.view(began + 60.0);
        assert_eq!((view.course_deg, view.course_words), (None, None));
    }

    #[test]
    fn an_external_vario_drives_the_climb_and_reports_its_battery() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        let now = 2_000.0;

        // Two sentences split across three deliveries, one of them damaged.
        session.on_instrument_bytes(now, b"$LK8EX1,98725,99999,180,21,10".to_vec());
        session.on_instrument_bytes(
            now + 0.1,
            b"85,*2D\r\n$LK8EX1,98725,99999,180,21,1085,*FF\r\n".to_vec(),
        );
        session.on_instrument_bytes(now + 0.2, b"$POV,E,2.10\r\n".to_vec());

        let view = session.view(now + 0.5);
        assert_eq!(view.climb_ms, Some(2.1));
        assert_eq!(view.vario_source, Some(VarioSourceKind::Instrument));
        let instrument = view.instrument.unwrap();
        assert!(instrument.talking);
        assert_eq!(instrument.battery_percent, Some(85));
        assert_eq!((instrument.sentences, instrument.damaged), (2, 1));

        // While the instrument supplies pressure the phone's is ignored;
        // once it has been silent the phone takes over.
        session.on_pressure(now + 1.0, 800.0);
        assert!(session.view(now + 1.0).altitude_m.is_none());
        let later = now + 10.0;
        session.on_pressure(later, 800.0);
        session.on_pressure(later + 1.0, 800.0);
        let view = session.view(later + 1.0);
        assert!(!view.instrument.unwrap().talking);
        assert_eq!(view.vario_source, Some(VarioSourceKind::Pressure));
    }

    #[test]
    fn the_vario_is_silent_on_the_ground_even_if_the_air_pressure_falls() {
        let folder = TempFolder::new();
        let session = FlightSession::new(folder_text(&folder));
        session.on_instrument_bytes(500.0, b"$POV,E,3.0\r\n".to_vec());
        let view = session.view(500.5);
        assert_eq!(view.climb_ms, Some(3.0));
        assert_eq!(view.phase, FlightPhase::Ground);
        assert_eq!(view.tone, None);
    }

    #[test]
    fn the_wind_says_where_it_comes_from_and_the_advanced_readings_wait_to_be_asked_for() {
        let folder = TempFolder::new();
        // On the glide after the first thermal, a minute after its last
        // circle: a westerly of 3 m/s.
        let (session, replay) = fly(&folder, 400.0, false);
        let view = session.view(replay.now());
        assert_eq!(view.wind_source, Some(WindSourceKind::Circling));
        let words = view.wind_words.clone().unwrap();
        assert!(
            words.contains(" km/h from W, from your last circles, ") && words.ends_with(" old"),
            "{words}"
        );
        // The glider through the air, estimated and said to be.
        let airspeed = view.airspeed_ms.unwrap();
        assert!((6.0..16.0).contains(&airspeed), "{airspeed}");
        assert!(view.airspeed_words.clone().unwrap().starts_with("about "));
        assert!(view.heading_words.clone().unwrap().starts_with("about "));
        assert_eq!(view.carried_back, None);
        assert_eq!(
            view.altitude_source_words.as_deref(),
            Some("Altitude from the pressure sensor, levelled to GPS")
        );
        // No terrain under the take-off: no QNH to say.
        assert_eq!((view.qnh_hpa, view.qnh_words.clone()), (None, None));
        assert!(!view.never_lands);

        // Without a glider class or a polar set, and without landings
        // handed over: nothing of netto, speed to fly or a landing.
        assert_eq!(view.netto_ms, None);
        assert_eq!(view.netto_words, None);
        assert_eq!(view.speed_to_fly, None);
        assert_eq!(view.nearest_landing, None);

        // The pilot sets a class: the air's own movement and a hint.
        session.set_glider_class(Some(GliderClassKind::B));
        let with_polar = session.view(replay.now());
        let netto = with_polar.netto_ms.unwrap();
        assert!(netto.abs() < 2.0, "{netto}");
        assert!(with_polar.netto_words.clone().unwrap().starts_with("air "));
        let hint = with_polar.speed_to_fly.clone().unwrap();
        assert!(
            hint.words.starts_with("Trim") || hint.words.starts_with("Speed up"),
            "{hint:?}"
        );
        // A polar that is none changes nothing; a real one is taken.
        assert!(!session.set_polar(10.0, 1.2, 14.0, 1.5));
        assert!(session.set_polar(10.5, 1.15, 14.0, 2.2));
        session.set_glider_class(None);
        assert_eq!(session.view(replay.now()).speed_to_fly, None);

        // Landings handed over: the nearest within glide stands as a
        // reading from the next position on.
        let (here, altitude) = (view.position.unwrap(), view.altitude_m.unwrap());
        let place = |name: &str, east_km: f64, below_m: f64| TargetPlace {
            name: name.to_owned(),
            position: Coordinate {
                lat: here.lat,
                lon: here.lon + east_km / 75.0,
            },
            elevation_m: Some(altitude - below_m),
        };
        session.use_landings(vec![
            place("Far", 6.0, 900.0),
            place("Musterwiese", 2.0, 900.0),
        ]);
        replay.advance(session.clone(), 1.0);
        let landing = session.view(replay.now()).nearest_landing.unwrap();
        assert!(landing.within_glide, "{landing:?}");
        assert_eq!(landing.index, Some(1));
        assert!(
            landing.words.starts_with("Landing Musterwiese: 2.")
                && landing.words.contains(" km, +"),
            "{landing:?}"
        );
        // None within glide: a caution, with the one that misses by least.
        session.use_landings(vec![place("High", -4.0, 100.0)]);
        replay.advance(session.clone(), 1.0);
        let landing = session.view(replay.now()).nearest_landing.unwrap();
        assert!(!landing.within_glide);
        assert!(
            landing.words.starts_with("No landing within glide: High ")
                && landing.words.ends_with(" m short"),
            "{landing:?}"
        );
    }

    #[test]
    fn the_new_readings_have_their_words() {
        let estimate = live::WindEstimate {
            wind: live::Wind {
                from_deg: 268.0,
                speed_ms: 3.1,
            },
            airspeed_ms: 10.0,
            quality: 0.4,
            source: WindSource::Glide,
            measured_s: 1_000.0,
        };
        assert_eq!(
            wind_in_words(&estimate, 1_130.0),
            "11 km/h from W, from your last glide, 2 min old"
        );
        assert_eq!(
            wind_in_words(
                &live::WindEstimate {
                    source: WindSource::Circling,
                    ..estimate
                },
                1_020.0
            ),
            "11 km/h from W, from your last circles, under a minute old"
        );
        assert_eq!(
            carried_back_in_words(4.2 / 3.6),
            "The wind carries you back: 4 km/h backwards"
        );
        let polar = Polar::for_class(GliderClass::C);
        let words =
            |head: f64, air: f64| speed_to_fly_view(&polar.speed_to_fly(head, air, 0.0)).words;
        assert_eq!(words(7.0, -1.5), "Speed up: sinking air and head wind");
        assert_eq!(words(0.0, -2.0), "Speed up: sinking air");
        assert_eq!(words(9.0, 0.0), "Speed up: head wind");
        assert_eq!(words(0.0, 1.5), "Trim: you are in rising air");
        assert_eq!(words(-6.0, 0.0), "Trim: tail wind");
        assert_eq!(words(0.0, 0.0), "Trim");
    }
}

#[cfg(test)]
mod cable_tests {
    use super::*;

    /// The cables of the development mode, and the Wallbergbahn's lower
    /// end, where its line begins.
    fn watch() -> (CableWatch, GeoPoint) {
        let body = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../server/tests/golden/cables.json"
        ))
        .unwrap();
        let set = CableSet::from_answer(200, body).unwrap();
        let start = GeoPoint::new(47.6750127, 11.7767441).unwrap();
        (
            CableWatch {
                set: Some(set),
                ..CableWatch::default()
            },
            start,
        )
    }

    #[test]
    fn a_cable_ahead_is_warned_of_once_and_can_be_silenced() {
        let (mut watch, start) = watch();
        // 30 m above the ground at the gondola's lower end, flying on.
        watch.feed(
            1000.0,
            start,
            Some(30.0),
            Some(135.0),
            Some(10.0),
            None,
            None,
        );
        let warning = watch.warning.clone().unwrap();
        assert_eq!(warning.kind, CableKind::Gondola);
        assert!(
            warning.words.starts_with("Cable here: gondola"),
            "{}",
            warning.words
        );
        // High above it: nothing.
        watch.feed(
            1001.0,
            start,
            Some(400.0),
            Some(135.0),
            Some(10.0),
            None,
            None,
        );
        assert_eq!(watch.warning, None);
        // Without terrain every cable on the course counts.
        watch.feed(1002.0, start, None, Some(135.0), Some(10.0), None, None);
        assert!(watch.warning.is_some());
        assert!(watch.silenced.is_empty());
    }
}
