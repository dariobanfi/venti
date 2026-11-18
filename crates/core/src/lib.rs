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
//! So far the core can open a flight file. It grows with each product phase.

use std::sync::Arc;

use venti_flight::{igc, summarise};

uniffi::setup_scaffolding!();

/// The one error type the shells see.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    /// The text is not a usable flight file. The message says why, in words
    /// fit to show to the pilot.
    #[error("{message}")]
    InvalidFlight {
        /// Why the file could not be used.
        message: String,
    },
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
    /// The track itself.
    pub track: Vec<TrackPoint>,
    /// How many lines of the file could not be read and were skipped.
    pub skipped_lines: u32,
}

/// The application core. A shell creates one and keeps it for its lifetime.
#[derive(Debug, Default, uniffi::Object)]
pub struct Core;

#[uniffi::export]
impl Core {
    /// Creates the core.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    /// The version of the core, for the about screen and bug reports.
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// Reads a flight file and returns what a screen needs to show it.
    pub fn open_flight(&self, igc_text: String) -> Result<FlightView, CoreError> {
        let invalid = |message: &str| CoreError::InvalidFlight {
            message: message.to_owned(),
        };
        let file = igc::parse(&igc_text).map_err(|error| match error {
            igc::IgcError::NoDate => {
                invalid("This is not a flight file: it has no date. Flight files end in .igc.")
            }
            igc::IgcError::NoFixes => invalid("This flight file contains no positions."),
        })?;
        let summary = summarise(&file.fixes)
            .ok_or_else(|| invalid("This flight file contains no positions."))?;

        let track = file
            .fixes
            .iter()
            .map(|fix| TrackPoint {
                lat: fix.position.lat(),
                lon: fix.position.lon(),
                altitude_m: f64::from(fix.gps_altitude_m),
                seconds: u32::try_from((fix.time - summary.start).num_seconds()).unwrap_or(0),
            })
            .collect();

        // The domain's box keeps its edges private; read them back through
        // the corner points of the track's extent.
        let (south, west, north, east) = extent(&file.fixes);

        Ok(FlightView {
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
            skipped_lines: u32::try_from(file.skipped_lines).unwrap_or(u32::MAX),
        })
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

    #[test]
    fn opens_the_sample_flight() {
        let text = igc::write(&synthetic_flight());
        let view = Core::new().open_flight(text).unwrap();

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
    }

    #[test]
    fn explains_why_a_file_is_not_a_flight() {
        let error = Core::new().open_flight("hello".to_owned()).unwrap_err();
        assert!(error.to_string().contains("not a flight file"), "{error}");

        let error = Core::new()
            .open_flight("AXVE000\r\nHFDTE040726\r\n".to_owned())
            .unwrap_err();
        assert!(error.to_string().contains("no positions"), "{error}");
    }
}
