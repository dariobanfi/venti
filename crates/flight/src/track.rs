//! Views of a track for drawing: climb rate along it, an altitude profile
//! small enough for a chart, and an outline small enough for a thumbnail.
//!
//! Everything here works on satellite altitude, like the summary does.

use venti_domain::GeoPoint;

use crate::igc::Fix;

/// Metres per degree of latitude on the FAI sphere.
const M_PER_DEG_LAT: f64 = 111_194.9;

/// One point of an altitude profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfilePoint {
    /// Seconds since the first fix.
    pub seconds: i64,
    /// Altitude in metres.
    pub altitude_m: i32,
}

/// The altitude to use for each fix.
///
/// A fix the recorder marked as lacking a valid altitude takes the altitude
/// of the last valid fix before it, or of the first valid fix if none came
/// before. A recording with no valid altitude at all is returned as written.
pub fn altitudes(fixes: &[Fix]) -> Vec<i32> {
    let Some(first_valid) = fixes.iter().find(|fix| fix.valid) else {
        return fixes.iter().map(|fix| fix.gps_altitude_m).collect();
    };
    let mut last = first_valid.gps_altitude_m;
    fixes
        .iter()
        .map(|fix| {
            if fix.valid {
                last = fix.gps_altitude_m;
            }
            last
        })
        .collect()
}

/// The climb rate at each fix, in metres per second, averaged over a window
/// of `window_s` seconds centred on the fix.
///
/// Ten seconds smooths the second-to-second noise of satellite altitude and
/// is still short enough to show where a thermal began and ended. Negative
/// values are sink. A recorder that logs less often than the window is long
/// gets the rate between each fix's neighbours instead. A track too short to
/// span any time gets zeros.
pub fn climb_rates(fixes: &[Fix], window_s: i64) -> Vec<f64> {
    let altitudes = altitudes(fixes);
    let seconds: Vec<i64> = fixes
        .iter()
        .map(|fix| (fix.time - fixes[0].time).num_seconds())
        .collect();
    let half = window_s / 2;
    let last = fixes.len().saturating_sub(1);

    let (mut earliest, mut latest) = (0, 0);
    (0..fixes.len())
        .map(|index| {
            while seconds[earliest] < seconds[index] - half {
                earliest += 1;
            }
            while latest < last && seconds[latest + 1] <= seconds[index] + half {
                latest += 1;
            }
            // Always reach at least one fix back and one forward.
            let from = earliest.min(index.saturating_sub(1));
            let to = latest.max((index + 1).min(last));
            let span = seconds[to] - seconds[from];
            if span > 0 {
                f64::from(altitudes[to] - altitudes[from]) / span as f64
            } else {
                0.0
            }
        })
        .collect()
}

/// The altitude over time, reduced to at most `max_points` points.
///
/// A long flight has far more fixes than a chart has pixels. The track is cut
/// into equal runs of fixes and each run contributes its lowest and highest
/// point, in the order they occurred, so no peak or low point is lost.
pub fn altitude_profile(fixes: &[Fix], max_points: usize) -> Vec<ProfilePoint> {
    let altitudes = altitudes(fixes);
    let point = |index: usize| ProfilePoint {
        seconds: (fixes[index].time - fixes[0].time).num_seconds(),
        altitude_m: altitudes[index],
    };
    if fixes.len() <= max_points.max(2) {
        return (0..fixes.len()).map(point).collect();
    }

    let runs = (max_points / 2).max(1);
    let mut profile = Vec::with_capacity(runs * 2);
    for run in 0..runs {
        let start = run * fixes.len() / runs;
        let end = ((run + 1) * fixes.len() / runs).max(start + 1);
        let range = start..end.min(fixes.len());
        let lowest = range
            .clone()
            .min_by_key(|&index| altitudes[index])
            .expect("a run holds at least one fix");
        let highest = range
            .max_by_key(|&index| altitudes[index])
            .expect("a run holds at least one fix");
        profile.push(point(lowest.min(highest)));
        if lowest != highest {
            profile.push(point(lowest.max(highest)));
        }
    }
    profile
}

/// Seconds over which positions are averaged before the outline is drawn:
/// about one circle in a thermal.
const OUTLINE_SMOOTHING_S: i64 = 30;

/// The shape of the track, reduced to at most `max_points` points.
///
/// Positions are first averaged over half a minute, which turns the circles
/// of a thermal into the line the glider drifted along while climbing. Then
/// points are dropped where leaving them out moves the line the least (the
/// Ramer-Douglas-Peucker method), starting with a tolerance of a few
/// thousandths of the track's extent and loosening it until the outline is
/// small enough. The first and last points are the track's own.
///
/// This is what a thumbnail or an overview map should show. It is not the
/// track: nothing may be measured on it.
pub fn outline(fixes: &[Fix], max_points: usize) -> Vec<GeoPoint> {
    let (Some(first), Some(last)) = (fixes.first(), fixes.last()) else {
        return Vec::new();
    };
    if fixes.len() <= 2 {
        return fixes.iter().map(|fix| fix.position).collect();
    }
    let max_points = max_points.max(2);

    // Local flat coordinates in metres are accurate enough over one flight.
    let (lat0, lon0) = (first.position.lat(), first.position.lon());
    let m_per_deg_lon = M_PER_DEG_LAT * lat0.to_radians().cos();
    let flat: Vec<(f64, f64)> = fixes
        .iter()
        .map(|fix| {
            (
                (fix.position.lon() - lon0) * m_per_deg_lon,
                (fix.position.lat() - lat0) * M_PER_DEG_LAT,
            )
        })
        .collect();
    let seconds: Vec<i64> = fixes
        .iter()
        .map(|fix| (fix.time - first.time).num_seconds())
        .collect();

    let smooth = smoothed(&flat, &seconds, OUTLINE_SMOOTHING_S / 2);

    let (mut min, mut max) = (smooth[0], smooth[0]);
    for &(x, y) in &smooth {
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    }
    let extent = (max.0 - min.0).hypot(max.1 - min.1);

    let mut tolerance = (extent * 0.004).max(15.0);
    let kept = loop {
        let kept = simplify(&smooth, tolerance);
        if kept.len() <= max_points {
            break kept;
        }
        tolerance *= 1.6;
    };

    let end = fixes.len() - 1;
    kept.into_iter()
        .map(|index| match index {
            0 => first.position,
            index if index == end => last.position,
            index => {
                let (x, y) = smooth[index];
                GeoPoint::new(lat0 + y / M_PER_DEG_LAT, lon0 + x / m_per_deg_lon)
                    .expect("an average of positions on the track is a valid position")
            }
        })
        .collect()
}

/// Each point replaced by the average of the points within `half_window_s`
/// seconds of it. `seconds` must not decrease.
fn smoothed(points: &[(f64, f64)], seconds: &[i64], half_window_s: i64) -> Vec<(f64, f64)> {
    let (mut from, mut to) = (0, 0);
    let mut sum = points[0];
    (0..points.len())
        .map(|index| {
            while to + 1 < points.len() && seconds[to + 1] <= seconds[index] + half_window_s {
                to += 1;
                sum = (sum.0 + points[to].0, sum.1 + points[to].1);
            }
            while seconds[from] < seconds[index] - half_window_s {
                sum = (sum.0 - points[from].0, sum.1 - points[from].1);
                from += 1;
            }
            let count = (to - from + 1) as f64;
            (sum.0 / count, sum.1 / count)
        })
        .collect()
}

/// The corners of the track: the positions, in the list of fixes, of at most
/// `max_points` fixes between which the track runs nearly straight. The
/// first and last fix are always among them.
///
/// Unlike [`outline`] nothing is smoothed and every point returned is a fix
/// of the track, so distances between corners are distances the glider
/// really covered.
pub fn corners(fixes: &[Fix], max_points: usize) -> Vec<usize> {
    let Some(first) = fixes.first() else {
        return Vec::new();
    };
    if fixes.len() <= 2 {
        return (0..fixes.len()).collect();
    }
    let (lat0, lon0) = (first.position.lat(), first.position.lon());
    let m_per_deg_lon = M_PER_DEG_LAT * lat0.to_radians().cos();
    let flat: Vec<(f64, f64)> = fixes
        .iter()
        .map(|fix| {
            (
                (fix.position.lon() - lon0) * m_per_deg_lon,
                (fix.position.lat() - lat0) * M_PER_DEG_LAT,
            )
        })
        .collect();

    let mut tolerance = 20.0;
    loop {
        let kept = simplify(&flat, tolerance);
        if kept.len() <= max_points.max(2) {
            return kept;
        }
        tolerance *= 1.5;
    }
}

/// The indices of the points Ramer-Douglas-Peucker keeps, in order.
fn simplify(points: &[(f64, f64)], tolerance: f64) -> Vec<usize> {
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;

    let mut pending = vec![(0, points.len() - 1)];
    while let Some((start, end)) = pending.pop() {
        let mut farthest = (0.0, start);
        for index in start + 1..end {
            let distance = distance_to_segment(points[index], points[start], points[end]);
            if distance > farthest.0 {
                farthest = (distance, index);
            }
        }
        if farthest.0 > tolerance {
            keep[farthest.1] = true;
            pending.push((start, farthest.1));
            pending.push((farthest.1, end));
        }
    }

    (0..points.len()).filter(|&index| keep[index]).collect()
}

fn distance_to_segment(point: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length_squared = dx * dx + dy * dy;
    let along = if length_squared == 0.0 {
        0.0
    } else {
        (((point.0 - a.0) * dx + (point.1 - a.1) * dy) / length_squared).clamp(0.0, 1.0)
    };
    (point.0 - (a.0 + along * dx)).hypot(point.1 - (a.1 + along * dy))
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};

    use super::*;
    use crate::testing::synthetic_flight;

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
    fn a_fix_without_a_valid_altitude_takes_its_neighbours() {
        let mut fixes = track([1000, 1010, 0, 1030]);
        fixes[0].valid = false;
        fixes[2].valid = false;
        assert_eq!(altitudes(&fixes), [1010, 1010, 1010, 1030]);
    }

    #[test]
    fn climb_rates_follow_a_climb_and_a_sink() {
        // 60 s climbing at 2 m/s, then 60 s sinking at 3 m/s.
        let climb = (0..=60).map(|s| 1000 + 2 * s);
        let sink = (1..=60).map(|s| 1120 - 3 * s);
        let rates = climb_rates(&track(climb.chain(sink)), 10);

        assert_eq!(rates.len(), 121);
        assert!((rates[30] - 2.0).abs() < 1e-9, "{}", rates[30]);
        assert!((rates[100] + 3.0).abs() < 1e-9, "{}", rates[100]);
        // The ends use the part of the window that exists.
        assert!((rates[0] - 2.0).abs() < 1e-9, "{}", rates[0]);
        assert!((rates[120] + 3.0).abs() < 1e-9, "{}", rates[120]);
    }

    #[test]
    fn a_recorder_slower_than_the_window_still_gets_climb_rates() {
        // One fix every ten seconds, sinking 1 m between fixes.
        let mut fixes = track([1620, 1619, 1618]);
        for (index, fix) in fixes.iter_mut().enumerate() {
            fix.time += Duration::seconds(9 * index as i64);
        }
        let rates = climb_rates(&fixes, 10);
        assert!(
            rates.iter().all(|rate| (rate + 0.1).abs() < 1e-9),
            "{rates:?}"
        );
    }

    #[test]
    fn a_single_fix_has_no_climb_rate() {
        assert_eq!(climb_rates(&track([1000]), 10), [0.0]);
        assert!(climb_rates(&[], 10).is_empty());
    }

    #[test]
    fn a_short_track_keeps_every_point_of_its_profile() {
        let profile = altitude_profile(&track([1000, 1005, 1003]), 400);
        assert_eq!(
            profile,
            [
                ProfilePoint {
                    seconds: 0,
                    altitude_m: 1000
                },
                ProfilePoint {
                    seconds: 1,
                    altitude_m: 1005
                },
                ProfilePoint {
                    seconds: 2,
                    altitude_m: 1003
                },
            ]
        );
    }

    #[test]
    fn a_reduced_profile_keeps_the_extremes_and_stays_in_order() {
        let flight = synthetic_flight();
        let profile = altitude_profile(&flight.fixes, 200);

        assert!(profile.len() <= 200, "{}", profile.len());
        assert!(
            profile
                .windows(2)
                .all(|pair| pair[0].seconds < pair[1].seconds)
        );
        assert_eq!(profile.iter().map(|p| p.altitude_m).max(), Some(2117));
        assert_eq!(profile.iter().map(|p| p.altitude_m).min(), Some(786));
    }

    #[test]
    fn the_outline_is_small_and_keeps_both_ends() {
        let flight = synthetic_flight();
        let outline = outline(&flight.fixes, 96);

        assert_eq!(outline.first(), Some(&flight.fixes[0].position));
        assert_eq!(outline.last(), Some(&flight.fixes.last().unwrap().position));
        // Four glides and three thermals, each close to a straight line once
        // the circles are averaged away.
        assert!((6..=24).contains(&outline.len()), "{}", outline.len());
    }

    #[test]
    fn thermal_circles_become_the_line_the_glider_drifted_along() {
        // Twenty circles of 45 m radius, 28 s each, drifting east at 3 m/s.
        let start = Utc.with_ymd_and_hms(2026, 7, 1, 10, 0, 0).unwrap();
        let fixes: Vec<Fix> = (0..=20 * 28)
            .map(|second| {
                let angle = std::f64::consts::TAU * f64::from(second) / 28.0;
                let east = 3.0 * f64::from(second) - 45.0 * angle.cos();
                let north = 45.0 * angle.sin();
                Fix {
                    time: start + Duration::seconds(i64::from(second)),
                    position: GeoPoint::new(
                        47.0 + north / M_PER_DEG_LAT,
                        11.0 + east / (M_PER_DEG_LAT * 47.0_f64.to_radians().cos()),
                    )
                    .unwrap(),
                    gps_altitude_m: 1000 + second,
                    pressure_altitude_m: 0,
                    valid: true,
                }
            })
            .collect();

        let outline = outline(&fixes, 96);

        assert!(outline.len() <= 6, "{}", outline.len());
        // Away from the two ends, the outline stays near the drift line.
        for point in &outline[1..outline.len() - 1] {
            let north = (point.lat() - 47.0) * M_PER_DEG_LAT;
            assert!(north.abs() < 15.0, "{north}");
        }
    }

    #[test]
    fn corners_are_fixes_of_the_track_in_order_with_both_ends() {
        let flight = synthetic_flight();
        let corners = corners(&flight.fixes, 400);

        assert!(corners.len() <= 400);
        assert_eq!(corners.first(), Some(&0));
        assert_eq!(corners.last(), Some(&(flight.fixes.len() - 1)));
        assert!(corners.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(super::corners(&flight.fixes[..1], 400), [0]);
        assert!(super::corners(&[], 400).is_empty());
    }

    #[test]
    fn a_straight_line_reduces_to_its_ends() {
        let fixes = track((0..500).map(|_| 1000));
        assert_eq!(outline(&fixes, 96).len(), 2);
    }

    #[test]
    fn an_empty_track_has_no_outline() {
        assert!(outline(&[], 96).is_empty());
        assert!(altitude_profile(&[], 400).is_empty());
    }
}
