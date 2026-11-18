use venti_domain::{BoundingBox, Timestamp};

use crate::igc::Fix;

/// The window over which climb and sink rates are averaged, in seconds.
///
/// Satellite altitude is noisy from one second to the next; half a minute is
/// roughly one circle in a thermal and gives a figure pilots recognise.
const CLIMB_WINDOW_S: i64 = 30;

/// The headline numbers of a recorded track.
///
/// These describe the whole recording. Finding the take-off and landing
/// inside it, and scoring the flight, are separate steps that are not built
/// yet.
#[derive(Debug, Clone, PartialEq)]
pub struct FlightSummary {
    /// Time of the first fix.
    pub start: Timestamp,
    /// Time of the last fix.
    pub end: Timestamp,
    /// Distance along the track, metres, on the FAI sphere.
    pub track_length_m: f64,
    /// Highest satellite altitude, metres.
    pub max_altitude_m: i32,
    /// Lowest satellite altitude, metres.
    pub min_altitude_m: i32,
    /// Best climb rate averaged over half a minute, metres per second.
    pub max_climb_ms: f64,
    /// Strongest sink rate averaged over half a minute, metres per second, as
    /// a negative number.
    pub max_sink_ms: f64,
    /// The rectangle that contains the track.
    pub bounds: BoundingBox,
}

impl FlightSummary {
    /// Seconds from the first fix to the last.
    pub fn duration_s(&self) -> i64 {
        (self.end - self.start).num_seconds()
    }
}

/// Summarises a track. Returns `None` for an empty one.
///
/// Fixes the recorder marked as lacking a valid altitude still count toward
/// the distance but not toward altitudes and climb rates.
pub fn summarise(fixes: &[Fix]) -> Option<FlightSummary> {
    let (first, last) = (fixes.first()?, fixes.last()?);

    let track_length_m = fixes
        .windows(2)
        .map(|pair| pair[0].position.distance_m(pair[1].position))
        .sum();

    let (mut south, mut north) = (first.position.lat(), first.position.lat());
    let (mut west, mut east) = (first.position.lon(), first.position.lon());
    for fix in fixes {
        south = south.min(fix.position.lat());
        north = north.max(fix.position.lat());
        west = west.min(fix.position.lon());
        east = east.max(fix.position.lon());
    }
    let bounds = BoundingBox::new(south, west, north, east)
        .expect("edges taken from valid points are in order");

    let with_altitude: Vec<&Fix> = fixes.iter().filter(|fix| fix.valid).collect();
    // A recording with no valid altitude at all still gets a summary; fall
    // back to whatever altitudes it holds.
    let altitudes: Vec<&Fix> = if with_altitude.is_empty() {
        fixes.iter().collect()
    } else {
        with_altitude
    };
    let max_altitude_m = altitudes.iter().map(|fix| fix.gps_altitude_m).max()?;
    let min_altitude_m = altitudes.iter().map(|fix| fix.gps_altitude_m).min()?;

    // For each fix, compare with the latest fix at least one window earlier.
    let (mut max_climb_ms, mut max_sink_ms) = (0.0_f64, 0.0_f64);
    let mut earlier = 0;
    for (index, fix) in altitudes.iter().enumerate() {
        while earlier + 1 < index
            && (fix.time - altitudes[earlier + 1].time).num_seconds() >= CLIMB_WINDOW_S
        {
            earlier += 1;
        }
        let seconds = (fix.time - altitudes[earlier].time).num_seconds();
        if seconds >= CLIMB_WINDOW_S {
            let rate =
                f64::from(fix.gps_altitude_m - altitudes[earlier].gps_altitude_m) / seconds as f64;
            max_climb_ms = max_climb_ms.max(rate);
            max_sink_ms = max_sink_ms.min(rate);
        }
    }

    Some(FlightSummary {
        start: first.time,
        end: last.time,
        track_length_m,
        max_altitude_m,
        min_altitude_m,
        max_climb_ms,
        max_sink_ms,
        bounds,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use venti_domain::GeoPoint;

    use super::*;

    /// One fix a second, moving north at about 10 m/s, with the given altitudes.
    fn track(altitudes: impl IntoIterator<Item = i32>) -> Vec<Fix> {
        let start = Utc.with_ymd_and_hms(2026, 7, 1, 10, 0, 0).unwrap();
        altitudes
            .into_iter()
            .enumerate()
            .map(|(second, altitude)| Fix {
                time: start + Duration::seconds(second as i64),
                position: GeoPoint::new(47.0 + second as f64 * 0.00009, 11.0).unwrap(),
                gps_altitude_m: altitude,
                pressure_altitude_m: 0,
                valid: true,
            })
            .collect()
    }

    #[test]
    fn an_empty_track_has_no_summary() {
        assert_eq!(summarise(&[]), None);
    }

    #[test]
    fn a_steady_climb_then_sink_gives_the_expected_rates() {
        // 120 s climbing at 2 m/s, then 60 s sinking at 3 m/s.
        let climb = (0..=120).map(|s| 1000 + 2 * s);
        let sink = (1..=60).map(|s| 1240 - 3 * s);
        let summary = summarise(&track(climb.chain(sink))).unwrap();

        assert_eq!(summary.duration_s(), 180);
        assert_eq!(summary.max_altitude_m, 1240);
        assert_eq!(summary.min_altitude_m, 1000);
        assert!(
            (summary.max_climb_ms - 2.0).abs() < 1e-9,
            "{}",
            summary.max_climb_ms
        );
        assert!(
            (summary.max_sink_ms + 3.0).abs() < 1e-9,
            "{}",
            summary.max_sink_ms
        );
        // 180 steps of 0.00009 degrees of latitude, about 10 m each.
        assert!(
            (summary.track_length_m - 180.0 * 10.0075).abs() < 1.0,
            "{}",
            summary.track_length_m
        );
    }

    #[test]
    fn a_short_spike_does_not_count_as_a_climb() {
        // Level flight with one fix 50 m too high: averaged over half a
        // minute that is under 2 m/s, not 50 m/s.
        let mut altitudes = vec![1000; 90];
        altitudes[45] = 1050;
        let summary = summarise(&track(altitudes)).unwrap();
        assert!(summary.max_climb_ms < 2.0, "{}", summary.max_climb_ms);
        assert_eq!(summary.max_altitude_m, 1050);
    }

    #[test]
    fn fixes_without_a_valid_altitude_do_not_set_the_altitude_range() {
        let mut fixes = track([1000, 1001, 1002, 1003]);
        fixes[2].valid = false;
        fixes[2].gps_altitude_m = 0;
        let summary = summarise(&fixes).unwrap();
        assert_eq!(summary.min_altitude_m, 1000);
    }

    #[test]
    fn a_recording_shorter_than_the_window_has_no_climb_rate() {
        let summary = summarise(&track((0..10).map(|s| 1000 + 5 * s))).unwrap();
        assert_eq!(summary.max_climb_ms, 0.0);
    }
}
