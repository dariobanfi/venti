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
//! [`Replay`] feeds a recorded flight through a session at any speed. It is
//! how the cockpit is tried on the ground and how its screens are tested.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::DateTime;
use venti_domain::GeoPoint;
use venti_flight::igc::{self, Fix, IgcFile};
use venti_flight::live::{
    self, FlightComputer, Phase, PositionSample, VarioSource, atmosphere, tone,
};
use venti_sensors::{Battery, SentenceReader};

use crate::{Coordinate, CoreError};

/// How long an external instrument may be silent, in seconds, before it
/// counts as disconnected and the phone's own pressure sensor takes over.
const INSTRUMENT_SILENCE_S: f64 = 3.0;

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
pub struct WindView {
    /// The direction it comes from, degrees clockwise from north.
    pub from_deg: f64,
    /// Its speed in metres per second.
    pub speed_ms: f64,
}

/// The thermal the glider is circling in, or the last one it left.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ThermalView {
    /// Height gained in it, metres.
    pub gain_m: f64,
    /// Average climb in it, metres per second.
    pub climb_ms: f64,
}

/// One point of the track behind the glider.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
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
    /// Metres forward per metre of height lost. Absent when not losing
    /// height.
    pub glide_ratio: Option<f64>,
    /// The wind, from the last time the glider circled.
    pub wind: Option<WindView>,
    /// Whether the glider is circling in lift. The shell shows its thermal
    /// page while this is true.
    pub circling: bool,
    /// The thermal it is in, or the last one it left.
    pub thermal: Option<ThermalView>,
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
    last_position_s: Option<f64>,
}

/// Which sensor is supplying pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PressureSource {
    Phone,
    Instrument,
    Replay,
}

impl Inner {
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
                last_position_s: None,
            }),
        })
    }

    /// Takes a position from the phone.
    pub fn on_location(&self, sample: LocationSample) {
        let Ok(position) = GeoPoint::new(sample.lat, sample.lon) else {
            return;
        };
        let mut inner = self.lock();
        inner.computer.position(PositionSample {
            time_s: sample.time_unix_s,
            position,
            altitude_m: sample.altitude_m,
            ground_speed_ms: sample.ground_speed_ms.filter(|speed| *speed >= 0.0),
            course_deg: sample.course_deg.filter(|course| *course >= 0.0),
        });
        inner.last_position_s = Some(sample.time_unix_s);

        let pressure_altitude = inner.computer.state(sample.time_unix_s).pressure_altitude_m;
        if let Some(recorder) = inner.recorder.as_mut() {
            // A failed write must not stop the flight computer. The fix is
            // lost; the next one tries again.
            let _ = recorder.add(&sample, position, pressure_altitude);
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

    /// Starts writing fixes to a recording. Does nothing if one is running.
    /// The pilot's and the glider's names go into the flight file's header,
    /// if given.
    pub fn start_recording(
        &self,
        pilot: Option<String>,
        glider: Option<String>,
    ) -> Result<(), CoreError> {
        let mut inner = self.lock();
        if inner.recorder.is_none() {
            let recorder = Recorder::new(&inner.folder, header_text(pilot), header_text(glider));
            inner.recorder = Some(recorder.map_err(storage)?);
        }
        Ok(())
    }

    /// Stops recording and returns the flight file's text, or nothing if no
    /// fix was recorded. The file stays in the data folder until
    /// [`clear_recording`] is called, so the text is not lost if importing
    /// it fails.
    pub fn stop_recording(&self) -> Result<Option<PendingRecording>, CoreError> {
        let Some(recorder) = self.lock().recorder.take() else {
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

        CockpitView {
            phase: match state.phase {
                Phase::Ground => FlightPhase::Ground,
                Phase::Flying => FlightPhase::Flying,
                Phase::Landed => FlightPhase::Landed,
            },
            recording: inner.recorder.is_some(),
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
            glide_ratio: state.glide_ratio,
            wind: state.wind.map(|wind| WindView {
                from_deg: wind.from_deg,
                speed_ms: wind.speed_ms,
            }),
            circling: state.circling,
            thermal: state.thermal.map(|thermal| ThermalView {
                gain_m: thermal.gain_m,
                climb_ms: thermal.climb_ms,
            }),
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
        }
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
        assert_eq!(view.trail.len(), 181);
        assert!(view.position_age_s.unwrap() < 1.0);
        // Climbing in the air: the vario beeps.
        let tone = view.tone.unwrap();
        assert!(tone.frequency_hz > 700.0 && tone.duty < 1.0);
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
}
