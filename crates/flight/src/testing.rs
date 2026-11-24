//! A made-up flight for tests and for the sample file bundled with the apps.
//!
//! No real pilot flew it, so it can be shipped without anyone's permission.
//! It is fully deterministic: the same call always returns the same track.

use std::f64::consts::TAU;

use chrono::{Duration, NaiveDate};
use venti_domain::{GeoPoint, Timestamp};

use crate::igc::{Fix, IgcFile};

/// Metres per degree of latitude on the FAI sphere.
const M_PER_DEG_LAT: f64 = 111_194.9;

/// Where the made-up flight takes off: the Wallberg above the Tegernsee.
const TAKE_OFF: (f64, f64, f64) = (47.6642, 11.7967, 1620.0);

/// One stretch of a made-up flight.
pub enum Leg {
    /// Straight flight.
    Glide {
        /// How long, seconds.
        seconds: i64,
        /// Degrees clockwise from north.
        heading: f64,
        /// Over the ground, metres per second.
        speed: f64,
        /// Metres per second; negative is sinking.
        climb: f64,
    },
    /// Circles of 45 m radius, 28 s each, drifting east with a 3 m/s wind.
    /// Climbing in a thermal, or spiralling down when `climb` is negative.
    /// Seen from above the circles turn clockwise: to the right.
    Circles {
        /// How many circles.
        circles: i64,
        /// Metres per second; negative is sinking.
        climb: f64,
    },
}

/// The glider's state: metres east and north of the take-off, altitude, and
/// seconds since the start.
struct Glider {
    /// Where it took off: latitude, longitude.
    origin: (f64, f64),
    start: Timestamp,
    east: f64,
    north: f64,
    altitude: f64,
    second: i64,
    fixes: Vec<Fix>,
}

impl Glider {
    fn record(&mut self) {
        let (lat0, lon0) = self.origin;
        let lat = lat0 + self.north / M_PER_DEG_LAT;
        let lon = lon0 + self.east / (M_PER_DEG_LAT * lat0.to_radians().cos());
        self.fixes.push(Fix {
            time: self.start + Duration::seconds(self.second),
            position: GeoPoint::new(lat, lon).expect("the flight stays near its take-off"),
            gps_altitude_m: self.altitude.round() as i32,
            // A recorder's pressure altitude differs from the satellite one
            // by a slowly changing offset; a constant is enough here.
            pressure_altitude_m: (self.altitude - 35.0).round() as i32,
            valid: true,
        });
    }

    fn fly(&mut self, leg: &Leg) {
        match *leg {
            Leg::Glide {
                seconds,
                heading,
                speed,
                climb,
            } => {
                let heading = heading.to_radians();
                for _ in 0..seconds {
                    self.east += speed * heading.sin();
                    self.north += speed * heading.cos();
                    self.altitude += climb;
                    self.second += 1;
                    self.record();
                }
            }
            Leg::Circles { circles, climb } => {
                let (radius, period, wind) = (45.0, 28_i64, 3.0);
                let (centre_east, centre_north) = (self.east + radius, self.north);
                for step in 1..=circles * period {
                    let angle = TAU * step as f64 / period as f64;
                    self.east = centre_east + wind * step as f64 - radius * angle.cos();
                    self.north = centre_north + radius * angle.sin();
                    self.altitude += climb;
                    self.second += 1;
                    self.record();
                }
            }
        }
    }
}

/// A flight of about half an hour over the Tegernsee valley: a glide out
/// from the take-off, a climb in a thermal, a glide across the valley, a
/// second weaker climb, a run up the valley and back, and a spiral down to
/// just above the valley floor. One fix a second.
pub fn synthetic_flight() -> IgcFile {
    made_up_flight(&[
        Leg::Glide {
            seconds: 90,
            heading: 320.0,
            speed: 10.5,
            climb: -1.2,
        },
        Leg::Circles {
            circles: 9,
            climb: 2.4,
        },
        Leg::Glide {
            seconds: 380,
            heading: 300.0,
            speed: 11.0,
            climb: -1.1,
        },
        Leg::Circles {
            circles: 6,
            climb: 1.6,
        },
        Leg::Glide {
            seconds: 300,
            heading: 200.0,
            speed: 10.5,
            climb: -1.2,
        },
        Leg::Glide {
            seconds: 330,
            heading: 20.0,
            speed: 10.5,
            climb: -1.2,
        },
        Leg::Circles {
            circles: 8,
            climb: -1.9,
        },
    ])
}

/// A made-up flight of the legs given, from the made-up flight's take-off
/// on the same day and with the same headers. One fix a second.
pub fn made_up_flight(legs: &[Leg]) -> IgcFile {
    let date = NaiveDate::from_ymd_opt(2026, 7, 4).expect("valid date");
    let start = date.and_hms_opt(10, 30, 0).expect("valid time").and_utc();
    made_up_flight_from(TAKE_OFF, start, legs)
}

/// A made-up flight of the legs given, from another take-off (latitude,
/// longitude, altitude in metres) and starting at `start`, with the made-up
/// flight's headers. One fix a second.
pub fn made_up_flight_from(take_off: (f64, f64, f64), start: Timestamp, legs: &[Leg]) -> IgcFile {
    let (lat, lon, altitude) = take_off;
    let date = start.date_naive();
    let mut glider = Glider {
        origin: (lat, lon),
        start,
        east: 0.0,
        north: 0.0,
        altitude,
        second: 0,
        fixes: Vec::new(),
    };
    glider.record();

    for leg in legs {
        glider.fly(leg);
    }

    IgcFile {
        manufacturer: "XVE".to_owned(),
        recorder_id: "000".to_owned(),
        date,
        pilot: Some("Sample Pilot".to_owned()),
        glider_type: Some("Sample Wing".to_owned()),
        glider_id: None,
        fixes: glider.fixes,
        skipped_lines: 0,
    }
}

/// The same flight mirrored east to west about its first fix: every turn
/// to the right becomes one to the left, and the wind blows the other way.
pub fn mirrored(flight: &IgcFile) -> IgcFile {
    let Some(first) = flight.fixes.first() else {
        return flight.clone();
    };
    let lon0 = first.position.lon();
    IgcFile {
        fixes: flight
            .fixes
            .iter()
            .map(|fix| Fix {
                position: GeoPoint::new(fix.position.lat(), 2.0 * lon0 - fix.position.lon())
                    .expect("the flight stays near its take-off"),
                ..*fix
            })
            .collect(),
        ..flight.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{igc, summarise};

    #[test]
    fn the_made_up_flight_has_the_shape_it_claims() {
        let flight = synthetic_flight();
        let summary = summarise(&flight.fixes).unwrap();

        // 90 + 9*28 + 380 + 6*28 + 300 + 330 + 8*28 seconds.
        assert_eq!(summary.duration_s(), 1744);
        assert_eq!(flight.fixes.len(), 1745);
        // The first thermal climbs at 2.4 m/s; nothing climbs faster. The
        // final spiral sinks at 1.9 m/s; nothing sinks faster.
        assert!(
            (summary.max_climb_ms - 2.4).abs() < 0.05,
            "{}",
            summary.max_climb_ms
        );
        assert!(
            (summary.max_sink_ms + 1.9).abs() < 0.05,
            "{}",
            summary.max_sink_ms
        );
        // Top of the first climb: 1620 - 90*1.2 + 252*2.4.
        assert_eq!(summary.max_altitude_m, 2117);
        // It ends just above the valley floor at Rottach-Egern, about 740 m.
        assert_eq!(summary.min_altitude_m, 786);
        assert!((12_000.0..22_000.0).contains(&summary.track_length_m));
    }

    #[test]
    fn the_sample_file_bundled_with_the_ios_app_is_this_flight() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/ios/Venti/Resources/sample.igc"
        );
        let bundled = std::fs::read_to_string(path).expect("the bundled sample exists");
        assert!(
            bundled == igc::write(&synthetic_flight()),
            "apps/ios/Venti/Resources/sample.igc is out of date; regenerate it with\n\
             cargo run -p venti-flight --example sample_flight > apps/ios/Venti/Resources/sample.igc"
        );
    }

    #[test]
    fn it_survives_being_written_and_read_back() {
        let flight = synthetic_flight();
        let read_back = igc::parse(&igc::write(&flight)).unwrap();
        assert_eq!(read_back.fixes.len(), flight.fixes.len());
        assert_eq!(read_back.skipped_lines, 0);
        assert_eq!(read_back.pilot, flight.pilot);
        let (a, b) = (
            summarise(&flight.fixes).unwrap(),
            summarise(&read_back.fixes).unwrap(),
        );
        assert_eq!(a.max_altitude_m, b.max_altitude_m);
        // The file keeps positions to a thousandth of a minute, under 2 m.
        assert!((a.track_length_m - b.track_length_m).abs() < 0.02 * a.track_length_m);
    }
}
