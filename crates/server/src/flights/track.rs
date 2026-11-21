//! A flight's track as data: every fix of the recording as a GeoJSON
//! line, for programs that are not Venti's apps and do not read IGC.
//!
//! The file is the truth and is handed out as it was uploaded
//! (`GET /v1/flights/{id}/igc`). This is the same recording in a form any
//! map library reads: nothing is judged, smoothed or left out, but for the
//! bound on its size.

use venti_api_types::v1::{FlightId, FlightTrack, MAX_TRACK_POINTS, TrackLine, TrackProperties};
use venti_flight::igc;
use venti_store::Derived;

use super::derive::{round, text_of};

/// The track of a stored flight, from its file as it was uploaded.
///
/// The line holds every fix of the recording, those before the take-off
/// and after the landing too: `derived` says where the analysis found the
/// two. A recording of more than [`MAX_TRACK_POINTS`] fixes is thinned to
/// every second, third, … fix, evenly, so that the line never holds more;
/// its last fix is always there. Such a recording is longer than thirteen
/// hours at a fix a second.
///
/// Reading a file is quick, a few milliseconds for a long flight, and has
/// no part that a file can make slow: the analysis is not run here.
///
/// `None` for a file that is no flight file, which no stored file is.
pub fn track(id: FlightId, file: &[u8], derived: &Derived) -> Option<FlightTrack> {
    let recording = igc::parse(&text_of(file)).ok()?;
    let fixes = &recording.fixes;
    let (first, last) = (fixes.first()?, fixes.len() - 1);
    let every = fixes.len().div_ceil(MAX_TRACK_POINTS).max(1);
    let kept: Vec<&igc::Fix> = fixes
        .iter()
        .enumerate()
        .filter(|(index, _)| index % every == 0 || *index == last)
        .map(|(_, fix)| fix)
        .collect();

    let since_start = |time: venti_domain::Timestamp| (time - first.time).num_seconds();
    Some(FlightTrack {
        kind: "Feature".to_owned(),
        id,
        geometry: TrackLine {
            kind: "LineString".to_owned(),
            // A flight file gives positions to a thousandth of a minute,
            // under two metres. Six decimals of a degree keep all of that.
            coordinates: kept
                .iter()
                .map(|fix| {
                    [
                        round(fix.position.lon(), 1e6),
                        round(fix.position.lat(), 1e6),
                        f64::from(fix.gps_altitude_m),
                    ]
                })
                .collect(),
        },
        properties: TrackProperties {
            start: first.time,
            times_s: kept.iter().map(|fix| since_start(fix.time)).collect(),
            pressure_altitudes_m: recording
                .has_pressure_altitude()
                .then(|| kept.iter().map(|fix| fix.pressure_altitude_m).collect()),
            fixes: fixes.len(),
            every,
            takeoff_s: derived.takeoff.map(|place| since_start(place.time)),
            landing_s: derived.landing.map(|place| since_start(place.time)),
        },
        terms: None,
    })
}

#[cfg(test)]
mod tests {
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::flights::derive::derive;

    fn id() -> FlightId {
        "0123456789abcdefghjkmnpqrs".parse().unwrap()
    }

    #[test]
    fn a_track_holds_every_fix_of_the_recording_with_its_time_and_heights() {
        let sample = synthetic_flight();
        let file = igc::write(&sample).into_bytes();
        let derived = derive(&file).unwrap().derived;

        let track = track(id(), &file, &derived).unwrap();

        let fixes = &sample.fixes;
        assert_eq!(track.kind, "Feature");
        assert_eq!(track.geometry.kind, "LineString");
        assert_eq!(track.geometry.coordinates.len(), fixes.len());
        assert_eq!(track.properties.times_s.len(), fixes.len());
        assert_eq!(track.properties.fixes, fixes.len());
        assert_eq!(track.properties.every, 1);
        assert_eq!(track.properties.start, fixes[0].time);
        assert_eq!(track.properties.times_s[0], 0);
        let last = fixes.last().unwrap();
        assert_eq!(
            *track.properties.times_s.last().unwrap(),
            (last.time - fixes[0].time).num_seconds()
        );
        // Longitude first, as the format has it, then latitude and height.
        let [lon, lat, altitude] = track.geometry.coordinates[0];
        assert!((lon - fixes[0].position.lon()).abs() < 1e-5, "{lon}");
        assert!((lat - fixes[0].position.lat()).abs() < 1e-5, "{lat}");
        assert_eq!(altitude, f64::from(fixes[0].gps_altitude_m));
        let pressure = track.properties.pressure_altitudes_m.unwrap();
        assert_eq!(pressure.len(), fixes.len());
        assert_eq!(pressure[0], fixes[0].pressure_altitude_m);
        // Where the analysis found the take-off and the landing.
        let takeoff = derived.takeoff.unwrap();
        assert_eq!(
            track.properties.takeoff_s,
            Some((takeoff.time - fixes[0].time).num_seconds())
        );
        assert_eq!(
            track.properties.landing_s,
            derived
                .landing
                .map(|landing| (landing.time - fixes[0].time).num_seconds())
        );
        assert_eq!(track.terms, None);
    }

    #[test]
    fn a_recording_longer_than_the_bound_is_thinned_evenly_and_keeps_its_last_fix() {
        // The sample flight again and again, a second apart, until the
        // recording is two and a half times the bound.
        let mut long = synthetic_flight();
        let pattern = long.fixes.clone();
        let wanted = MAX_TRACK_POINTS * 5 / 2 + 7;
        let first = pattern[0].time;
        long.fixes = (0..wanted)
            .map(|n| igc::Fix {
                time: first + chrono::Duration::seconds(n as i64),
                ..pattern[n % pattern.len()]
            })
            .collect();
        // Every third fix, and the last.
        let file = igc::write(&long).into_bytes();
        let derived = derive(&igc::write(&synthetic_flight()).into_bytes())
            .unwrap()
            .derived;

        let track = track(id(), &file, &derived).unwrap();

        assert_eq!(track.properties.fixes, wanted);
        assert_eq!(track.properties.every, 3);
        let points = track.geometry.coordinates.len();
        assert!(points <= MAX_TRACK_POINTS, "{points}");
        assert_eq!(points, wanted.div_ceil(3) + 1);
        assert_eq!(track.properties.times_s.len(), points);
        assert_eq!(&track.properties.times_s[..3], [0, 3, 6]);
        assert_eq!(*track.properties.times_s.last().unwrap(), wanted as i64 - 1);
    }

    #[test]
    fn what_is_no_flight_file_has_no_track() {
        let derived = derive(&igc::write(&synthetic_flight()).into_bytes())
            .unwrap()
            .derived;
        assert_eq!(track(id(), b"not a flight", &derived), None);
        assert_eq!(track(id(), b"", &derived), None);
    }
}
