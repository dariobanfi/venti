//! The client application core.
//!
//! The iOS, Android and web shells call this crate and nothing below it. It
//! owns everything that is not pixels or an operating-system service, so the
//! three apps behave identically.
//!
//! Rules for what is exposed here:
//!
//! - The interface is coarse. A screen asks for one view of what it shows and
//!   gets one value back, not a dozen getters.
//! - Values that cross the boundary are plain data in units the shells can
//!   format directly: metres, seconds, degrees.
//! - Nothing per-frame or per-audio-sample crosses the boundary.
//! - One error type crosses the boundary.
//!
//! So far the core keeps the pilot's logbook on the device: it imports flight
//! files, lists them and prepares one flight for display. It grows with each
//! product phase.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Datelike};
use venti_flight::{igc, summarise, track};

mod logbook;
#[cfg(test)]
mod testing;

use logbook::{Logbook, LogbookError};

uniffi::setup_scaffolding!();

/// The window, in seconds, over which the climb rate along a track is
/// averaged for display.
const TRACK_CLIMB_WINDOW_S: i64 = 10;

/// The most points an altitude profile holds: about one per pixel of a
/// phone-width chart.
const PROFILE_POINTS: usize = 400;

/// The one error type the shells see. Every message is fit to show to the
/// pilot as it is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    /// The text is not a usable flight file. The message says why.
    #[error("{message}")]
    InvalidFlight {
        /// Why the file could not be used.
        message: String,
    },
    /// The logbook holds no flight with the identifier that was asked for.
    #[error("{message}")]
    UnknownFlight {
        /// What happened.
        message: String,
    },
    /// The data folder could not be read or written.
    #[error("{message}")]
    Storage {
        /// What happened.
        message: String,
    },
}

impl From<LogbookError> for CoreError {
    fn from(error: LogbookError) -> Self {
        match error {
            LogbookError::Invalid(igc::IgcError::NoDate) => Self::InvalidFlight {
                message: "This is not a flight file: it has no date. Flight files end in .igc."
                    .to_owned(),
            },
            LogbookError::Invalid(igc::IgcError::NoFixes) => Self::InvalidFlight {
                message: "This flight file contains no positions.".to_owned(),
            },
            LogbookError::Unknown => Self::UnknownFlight {
                message: "This flight is no longer in the logbook.".to_owned(),
            },
            LogbookError::Storage(error) => Self::Storage {
                message: format!("The logbook could not be read or written: {error}."),
            },
        }
    }
}

/// A place on the ground.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Coordinate {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
}

/// One point of a track, for drawing.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct TrackPoint {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
    /// Altitude in metres.
    pub altitude_m: f64,
    /// Seconds since the first point.
    pub seconds: u32,
    /// Climb rate around this point averaged over ten seconds, metres per
    /// second. Negative is sink.
    pub climb_ms: f64,
}

/// One point of an altitude profile, for a chart.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ProfilePoint {
    /// Seconds since the first point of the track.
    pub seconds: u32,
    /// Altitude in metres.
    pub altitude_m: f64,
}

/// A rectangle in latitude and longitude.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Bounds {
    /// Southern edge, degrees.
    pub south: f64,
    /// Western edge, degrees.
    pub west: f64,
    /// Northern edge, degrees.
    pub north: f64,
    /// Eastern edge, degrees.
    pub east: f64,
}

/// Everything a screen needs to show one recorded flight.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FlightView {
    /// The flight's identifier in the logbook.
    pub id: String,
    /// The pilot's name, if the file gives one.
    pub pilot: Option<String>,
    /// The glider model, if the file gives one.
    pub glider: Option<String>,
    /// The recorder's maker code and serial.
    pub recorder: String,
    /// Time of the first fix, seconds since 1970-01-01 UTC.
    pub start_unix_s: i64,
    /// Seconds from the first fix to the last.
    pub duration_s: i64,
    /// Distance along the track in metres.
    pub track_length_m: f64,
    /// Highest altitude in metres.
    pub max_altitude_m: f64,
    /// Lowest altitude in metres.
    pub min_altitude_m: f64,
    /// Best climb rate averaged over half a minute, metres per second.
    pub max_climb_ms: f64,
    /// Strongest sink rate averaged over half a minute, metres per second,
    /// as a negative number.
    pub max_sink_ms: f64,
    /// The rectangle that contains the track.
    pub bounds: Bounds,
    /// The track itself, every fix.
    pub track: Vec<TrackPoint>,
    /// The altitude over time, reduced to what a chart can show. It keeps
    /// every high and low point.
    pub profile: Vec<ProfilePoint>,
    /// How many lines of the file could not be read and were skipped.
    pub skipped_lines: u32,
}

/// The result of importing a flight file.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ImportedFlight {
    /// The flight's identifier in the logbook.
    pub id: String,
    /// Whether the logbook already held this flight. Nothing was stored then.
    pub already_in_logbook: bool,
}

/// Flights counted together.
#[derive(Debug, Clone, Copy, Default, PartialEq, uniffi::Record)]
pub struct Totals {
    /// Number of flights.
    pub flights: u32,
    /// Time in the air, seconds.
    pub airtime_s: i64,
    /// Distance along the tracks, metres.
    pub track_length_m: f64,
}

/// One line of the logbook.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LogbookEntry {
    /// The flight's identifier, to open or delete it.
    pub id: String,
    /// Time of the first fix, seconds since 1970-01-01 UTC.
    pub start_unix_s: i64,
    /// Seconds from the first fix to the last.
    pub duration_s: i64,
    /// Distance along the track in metres.
    pub track_length_m: f64,
    /// Highest altitude in metres.
    pub max_altitude_m: f64,
    /// Best climb rate averaged over half a minute, metres per second.
    pub max_climb_ms: f64,
    /// The pilot's name, if the file gives one.
    pub pilot: Option<String>,
    /// The glider model, if the file gives one.
    pub glider: Option<String>,
    /// The shape of the track in a few dozen points, for a thumbnail or an
    /// overview map.
    pub outline: Vec<Coordinate>,
}

/// The flights of one calendar year (UTC), newest first.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LogbookYear {
    /// The year.
    pub year: i32,
    /// The year's flights counted together.
    pub totals: Totals,
    /// The flights.
    pub flights: Vec<LogbookEntry>,
}

/// Everything the logbook screen shows.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LogbookView {
    /// All flights counted together.
    pub totals: Totals,
    /// The flights by year, newest year first.
    pub years: Vec<LogbookYear>,
}

/// The application core. A shell creates one and keeps it for its lifetime.
#[derive(Debug, uniffi::Object)]
pub struct Core {
    logbook: Logbook,
}

#[uniffi::export]
impl Core {
    /// Creates the core. `data_folder` is a folder the app owns; the core
    /// keeps the pilot's flights there and creates it if it does not exist.
    #[uniffi::constructor]
    pub fn new(data_folder: String) -> Result<Arc<Self>, CoreError> {
        Ok(Arc::new(Self {
            logbook: Logbook::open(Path::new(&data_folder))?,
        }))
    }

    /// The version of the core, for the about screen and bug reports.
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// Adds a flight file to the logbook. Importing a flight that is already
    /// there changes nothing and says so.
    pub fn import_flight(&self, igc_text: String) -> Result<ImportedFlight, CoreError> {
        let (id, already_in_logbook) = self.logbook.import(&igc_text)?;
        Ok(ImportedFlight {
            id,
            already_in_logbook,
        })
    }

    /// The logbook: every flight grouped by year, with totals.
    pub fn logbook(&self) -> LogbookView {
        let mut view = LogbookView {
            totals: Totals::default(),
            years: Vec::new(),
        };
        for (id, summary) in self.logbook.flights() {
            let year = DateTime::from_timestamp(summary.start_unix_s, 0)
                .map_or(1970, |start| start.year());
            if view.years.last().is_none_or(|last| last.year != year) {
                view.years.push(LogbookYear {
                    year,
                    totals: Totals::default(),
                    flights: Vec::new(),
                });
            }
            let section = view.years.last_mut().expect("a year was just pushed");
            for totals in [&mut view.totals, &mut section.totals] {
                totals.flights += 1;
                totals.airtime_s += summary.duration_s;
                totals.track_length_m += summary.track_length_m;
            }
            section.flights.push(LogbookEntry {
                id,
                start_unix_s: summary.start_unix_s,
                duration_s: summary.duration_s,
                track_length_m: summary.track_length_m,
                max_altitude_m: f64::from(summary.max_altitude_m),
                max_climb_ms: summary.max_climb_ms,
                pilot: summary.pilot,
                glider: summary.glider,
                outline: summary
                    .outline
                    .into_iter()
                    .map(|[lat, lon]| Coordinate { lat, lon })
                    .collect(),
            });
        }
        view
    }

    /// One flight from the logbook, prepared for display.
    pub fn flight(&self, id: String) -> Result<FlightView, CoreError> {
        let text = self.logbook.flight_text(&id)?;
        let file = igc::parse(&text).map_err(LogbookError::Invalid)?;
        let summary =
            summarise(&file.fixes).ok_or(LogbookError::Invalid(igc::IgcError::NoFixes))?;

        let altitudes = track::altitudes(&file.fixes);
        let climbs = track::climb_rates(&file.fixes, TRACK_CLIMB_WINDOW_S);
        let seconds = |seconds: i64| u32::try_from(seconds).unwrap_or(0);
        let track = file
            .fixes
            .iter()
            .zip(altitudes.iter().zip(&climbs))
            .map(|(fix, (&altitude, &climb))| TrackPoint {
                lat: fix.position.lat(),
                lon: fix.position.lon(),
                altitude_m: f64::from(altitude),
                seconds: seconds((fix.time - summary.start).num_seconds()),
                climb_ms: climb,
            })
            .collect();
        let profile = track::altitude_profile(&file.fixes, PROFILE_POINTS)
            .into_iter()
            .map(|point| ProfilePoint {
                seconds: seconds(point.seconds),
                altitude_m: f64::from(point.altitude_m),
            })
            .collect();

        // The domain's box keeps its edges private; read them back through
        // the corner points of the track's extent.
        let (south, west, north, east) = extent(&file.fixes);

        Ok(FlightView {
            id,
            pilot: file.pilot,
            glider: file.glider_type,
            recorder: format!("{} {}", file.manufacturer, file.recorder_id)
                .trim()
                .to_owned(),
            start_unix_s: summary.start.timestamp(),
            duration_s: summary.duration_s(),
            track_length_m: summary.track_length_m,
            max_altitude_m: f64::from(summary.max_altitude_m),
            min_altitude_m: f64::from(summary.min_altitude_m),
            max_climb_ms: summary.max_climb_ms,
            max_sink_ms: summary.max_sink_ms,
            bounds: Bounds {
                south,
                west,
                north,
                east,
            },
            track,
            profile,
            skipped_lines: u32::try_from(file.skipped_lines).unwrap_or(u32::MAX),
        })
    }

    /// Removes a flight from the logbook for good.
    pub fn delete_flight(&self, id: String) -> Result<(), CoreError> {
        Ok(self.logbook.delete(&id)?)
    }
}

fn extent(fixes: &[igc::Fix]) -> (f64, f64, f64, f64) {
    let mut extent = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for fix in fixes {
        extent.0 = extent.0.min(fix.position.lat());
        extent.1 = extent.1.min(fix.position.lon());
        extent.2 = extent.2.max(fix.position.lat());
        extent.3 = extent.3.max(fix.position.lon());
    }
    extent
}

#[cfg(test)]
mod tests {
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    fn core(folder: &TempFolder) -> Arc<Core> {
        Core::new(folder.path().to_str().unwrap().to_owned()).unwrap()
    }

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    #[test]
    fn an_imported_flight_opens_with_everything_a_screen_shows() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let imported = core.import_flight(sample()).unwrap();
        assert!(!imported.already_in_logbook);

        let view = core.flight(imported.id.clone()).unwrap();

        assert_eq!(view.id, imported.id);
        assert_eq!(view.pilot.as_deref(), Some("Sample Pilot"));
        assert_eq!(view.glider.as_deref(), Some("Sample Wing"));
        assert_eq!(view.recorder, "XVE 000");
        assert_eq!(view.duration_s, 1744);
        assert_eq!(view.track.len(), 1745);
        assert_eq!(view.track[0].seconds, 0);
        assert_eq!(view.track.last().unwrap().seconds, 1744);
        assert_eq!(view.max_altitude_m, 2117.0);
        assert!(view.bounds.south < view.bounds.north && view.bounds.west < view.bounds.east);
        assert_eq!(view.skipped_lines, 0);
        // The first thermal of the sample climbs at 2.4 m/s; its glides sink
        // at a little over 1 m/s.
        let (best, worst) = view
            .track
            .iter()
            .fold((f64::MIN, f64::MAX), |(best, worst), point| {
                (best.max(point.climb_ms), worst.min(point.climb_ms))
            });
        assert!((best - 2.4).abs() < 0.11, "{best}");
        assert!((worst + 1.9).abs() < 0.11, "{worst}");
        assert!(view.profile.len() <= PROFILE_POINTS);
        assert_eq!(view.profile.first().unwrap().seconds, 0);
    }

    #[test]
    fn the_logbook_groups_flights_by_year_and_adds_them_up() {
        let folder = TempFolder::new();
        let core = core(&folder);
        assert_eq!(core.logbook().totals, Totals::default());
        assert!(core.logbook().years.is_empty());

        core.import_flight(sample()).unwrap();
        core.import_flight(sample().replace("040726", "030726"))
            .unwrap();
        core.import_flight(sample().replace("040726", "040725"))
            .unwrap();
        let again = core.import_flight(sample()).unwrap();
        assert!(again.already_in_logbook);

        let logbook = core.logbook();
        assert_eq!(logbook.totals.flights, 3);
        assert_eq!(logbook.totals.airtime_s, 3 * 1744);
        let years: Vec<(i32, u32)> = logbook
            .years
            .iter()
            .map(|year| (year.year, year.totals.flights))
            .collect();
        assert_eq!(years, [(2026, 2), (2025, 1)]);
        let newest = &logbook.years[0].flights;
        assert!(newest[0].start_unix_s > newest[1].start_unix_s);
        assert_eq!(newest[0].id, again.id);
        assert!(newest[0].outline.len() >= 2);
        assert!(
            (logbook.years[0].totals.track_length_m - 2.0 * newest[0].track_length_m).abs() < 1e-6
        );
    }

    #[test]
    fn a_deleted_flight_can_no_longer_be_opened() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let id = core.import_flight(sample()).unwrap().id;

        core.delete_flight(id.clone()).unwrap();

        assert!(core.logbook().years.is_empty());
        let error = core.flight(id).unwrap_err();
        assert!(
            matches!(error, CoreError::UnknownFlight { .. }),
            "{error:?}"
        );
        assert!(error.to_string().contains("no longer in the logbook"));
    }

    #[test]
    fn explains_why_a_file_is_not_a_flight() {
        let folder = TempFolder::new();
        let core = core(&folder);

        let error = core.import_flight("hello".to_owned()).unwrap_err();
        assert!(error.to_string().contains("not a flight file"), "{error}");

        let error = core
            .import_flight("AXVE000\r\nHFDTE040726\r\n".to_owned())
            .unwrap_err();
        assert!(error.to_string().contains("no positions"), "{error}");
    }

    #[test]
    fn a_data_folder_that_cannot_be_created_is_a_storage_error() {
        let folder = TempFolder::new();
        let file = folder.path().join("a-file");
        std::fs::write(&file, "x").unwrap();

        let error = Core::new(file.join("inside").to_str().unwrap().to_owned()).unwrap_err();
        assert!(matches!(error, CoreError::Storage { .. }), "{error:?}");
    }
}
