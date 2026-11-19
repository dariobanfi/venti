//! Scoring a flight the way cross-country leagues do.
//!
//! The rules are those of the World XContest as published at
//! xcontest.org/world/en/rules, read 2026-11-19:
//!
//! - **Free flight**: the longest distance through up to three turnpoints.
//!   One point per kilometre.
//! - **Triangle**: three turnpoints. It counts when the start and finish
//!   points lie closer together than 20% of the triangle's perimeter; the
//!   scoring distance is the perimeter minus that gap. A triangle whose
//!   shortest leg is at least 28% of the perimeter is an **FAI triangle**
//!   (1.4 points per kilometre), any other a **free triangle** (1.2). With
//!   a gap under 5% the triangle is **closed** and scores 1.6 or 1.4.
//! - The rule that gives the most points is used.
//!
//! To verify: that distances here, on the FAI sphere, match XContest's own
//! to its two decimals, and how its optimiser places start and finish. Until
//! a flight has been compared with XContest's result, the points are ours,
//! not the league's.

use crate::igc::Fix;
use crate::track;

/// The most corners the triangle search looks at. The search tries every
/// three of them, so its work grows with the cube of this number.
const TRIANGLE_POINTS: usize = 240;
/// The most corners the free-flight search looks at.
const FREE_POINTS: usize = 400;

/// What a flight is scored as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    /// The longest distance through up to three turnpoints.
    FreeFlight,
    /// A triangle that is not an FAI triangle.
    FreeTriangle,
    /// A triangle whose shortest leg is at least 28% of its perimeter.
    FaiTriangle,
}

impl RouteKind {
    /// Points per kilometre.
    pub fn points_per_km(self, closed: bool) -> f64 {
        match (self, closed) {
            (Self::FreeFlight, _) => 1.0,
            (Self::FreeTriangle, false) => 1.2,
            (Self::FreeTriangle, true) | (Self::FaiTriangle, false) => 1.4,
            (Self::FaiTriangle, true) => 1.6,
        }
    }
}

/// A flight's best score.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    /// What the flight is scored as.
    pub kind: RouteKind,
    /// For a triangle: whether start and finish lie within 5% of the
    /// perimeter of each other. Always false for a free flight.
    pub closed: bool,
    /// The scoring distance in metres: for a triangle, the perimeter minus
    /// the gap between start and finish.
    pub distance_m: f64,
    /// The points: kilometres times the points per kilometre.
    pub points: f64,
    /// The fixes the route runs through, as positions in the track, in
    /// flight order. A free flight has two to five: start, turnpoints,
    /// finish. A triangle has five: start, the three turnpoints, finish.
    pub route: Vec<usize>,
    /// For a triangle, the gap between start and finish in metres.
    pub closing_m: Option<f64>,
}

/// Scores a track. Give it the flown part of a recording. Returns `None`
/// for a track of fewer than two fixes.
pub fn score(fixes: &[Fix]) -> Option<Score> {
    let free = free_flight(fixes)?;
    match triangle(fixes) {
        Some(triangle) if triangle.points > free.points => Some(triangle),
        _ => Some(free),
    }
}

/// The longest path start, up to three turnpoints, finish, with the points
/// in flight order.
///
/// Trying every combination of five fixes is far too slow, so the search
/// runs on the track reduced to its corners: the best turnpoints of a path
/// are always at corners of the track. On those, the best path with one
/// more leg ending at each point is built from the best paths with one leg
/// fewer, four times over.
fn free_flight(fixes: &[Fix]) -> Option<Score> {
    if fixes.len() < 2 {
        return None;
    }
    let corners = track::corners(fixes, FREE_POINTS);
    let n = corners.len();
    let distance = |a: usize, b: usize| {
        fixes[corners[a]]
            .position
            .distance_m(fixes[corners[b]].position)
    };

    // best[i]: the longest path with the current number of legs that ends
    // at corner i; back[leg][i]: where its last leg came from.
    let mut best = vec![0.0_f64; n];
    let mut back: Vec<Vec<usize>> = Vec::new();
    for _leg in 0..4 {
        let mut next = vec![0.0_f64; n];
        let mut from = vec![0_usize; n];
        for end in 0..n {
            for (start, &so_far) in best.iter().enumerate().take(end + 1) {
                let length = so_far + distance(start, end);
                if length > next[end] {
                    next[end] = length;
                    from[end] = start;
                }
            }
        }
        best = next;
        back.push(from);
    }

    let (mut at, &distance_m) = best.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    let mut route = vec![corners[at]];
    for from in back.iter().rev() {
        at = from[at];
        route.push(corners[at]);
    }
    route.reverse();
    // Legs of no length are turnpoints the path did not need.
    route.dedup();
    Some(Score {
        kind: RouteKind::FreeFlight,
        closed: false,
        distance_m,
        points: distance_m / 1000.0,
        route,
        closing_m: None,
    })
}

/// The triangle that scores the most points, if the track closes one.
///
/// Every three corners of the track, in flight order, are tried as
/// turnpoints. The start may be any corner up to the first turnpoint and
/// the finish any corner from the third on; the pair that lies closest
/// together is found for all turnpoint pairs at once, before the search.
fn triangle(fixes: &[Fix]) -> Option<Score> {
    let corners = track::corners(fixes, TRIANGLE_POINTS);
    let n = corners.len();
    if n < 3 {
        return None;
    }
    let distance: Vec<Vec<f64>> = (0..n)
        .map(|a| {
            (0..n)
                .map(|b| {
                    fixes[corners[a]]
                        .position
                        .distance_m(fixes[corners[b]].position)
                })
                .collect()
        })
        .collect();

    // gap[a][c]: the smallest distance between a start at or before corner
    // a and a finish at or after corner c, and which corners they are.
    let mut gap = vec![vec![(f64::MAX, 0, 0); n]; n];
    for a in 0..n {
        for c in (a..n).rev() {
            let mut best = (distance[a][c], a, c);
            if a > 0 && gap[a - 1][c].0 < best.0 {
                best = gap[a - 1][c];
            }
            if c + 1 < n && gap[a][c + 1].0 < best.0 {
                best = gap[a][c + 1];
            }
            gap[a][c] = best;
        }
    }

    let mut best: Option<Score> = None;
    for a in 0..n {
        for c in a + 2..n {
            let (closing, start, finish) = gap[a][c];
            for b in a + 1..c {
                let legs = [distance[a][b], distance[b][c], distance[c][a]];
                let perimeter: f64 = legs.iter().sum();
                if closing >= 0.2 * perimeter {
                    continue;
                }
                let shortest = legs.iter().copied().fold(f64::MAX, f64::min);
                let kind = if shortest >= 0.28 * perimeter {
                    RouteKind::FaiTriangle
                } else {
                    RouteKind::FreeTriangle
                };
                let closed = closing < 0.05 * perimeter;
                let distance_m = perimeter - closing;
                let points = distance_m / 1000.0 * kind.points_per_km(closed);
                if best.as_ref().is_none_or(|score| points > score.points) {
                    best = Some(Score {
                        kind,
                        closed,
                        distance_m,
                        points,
                        route: vec![
                            corners[start],
                            corners[a],
                            corners[b],
                            corners[c],
                            corners[finish],
                        ],
                        closing_m: Some(closing),
                    });
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use venti_domain::GeoPoint;

    use super::*;
    use crate::testing::synthetic_flight;

    const M_PER_DEG_LAT: f64 = 111_194.9;

    /// A track flown in straight lines through points given in kilometres
    /// east and north of a place in the Alps, one fix every ten seconds at
    /// 36 km/h.
    fn track_through(points: &[(f64, f64)]) -> Vec<Fix> {
        let start = Utc.with_ymd_and_hms(2026, 7, 1, 10, 0, 0).unwrap();
        let position = |(east, north): (f64, f64)| {
            GeoPoint::new(
                47.0 + north * 1000.0 / M_PER_DEG_LAT,
                11.0 + east * 1000.0 / (M_PER_DEG_LAT * 47.0_f64.to_radians().cos()),
            )
            .unwrap()
        };
        let mut fixes = Vec::new();
        for leg in points.windows(2) {
            let (from, to) = (leg[0], leg[1]);
            let steps = ((to.0 - from.0).hypot(to.1 - from.1) * 10.0)
                .ceil()
                .max(1.0) as usize;
            for step in 0..steps {
                let part = step as f64 / steps as f64;
                fixes.push((
                    from.0 + (to.0 - from.0) * part,
                    from.1 + (to.1 - from.1) * part,
                ));
            }
        }
        fixes.push(*points.last().unwrap());
        fixes
            .into_iter()
            .enumerate()
            .map(|(index, point)| Fix {
                time: start + Duration::seconds(10 * index as i64),
                position: position(point),
                gps_altitude_m: 2000,
                pressure_altitude_m: 0,
                valid: true,
            })
            .collect()
    }

    #[test]
    fn a_flight_in_one_direction_is_a_free_flight() {
        // 30 km east with a dog-leg.
        let score = score(&track_through(&[
            (0.0, 0.0),
            (10.0, 4.0),
            (20.0, 0.0),
            (30.0, 3.0),
        ]))
        .unwrap();

        assert_eq!(score.kind, RouteKind::FreeFlight);
        let legs = 10.0_f64.hypot(4.0) + 10.0_f64.hypot(4.0) + 10.0_f64.hypot(3.0);
        assert!((score.distance_m / 1000.0 - legs).abs() < 0.05, "{score:?}");
        assert!((score.points - score.distance_m / 1000.0).abs() < 1e-9);
        assert_eq!(score.route.len(), 4);
        assert_eq!(score.closing_m, None);
    }

    #[test]
    fn a_closed_triangle_with_even_legs_is_a_closed_fai_triangle() {
        // 12 km, 13 km and about 12.4 km, back to the start.
        let fixes = track_through(&[(0.0, 0.0), (12.0, 0.0), (7.0, 12.0), (0.0, 0.0)]);
        let score = score(&fixes).unwrap();

        assert_eq!(score.kind, RouteKind::FaiTriangle);
        assert!(score.closed);
        let perimeter = 12.0 + 5.0_f64.hypot(12.0) + 7.0_f64.hypot(12.0);
        assert!(
            (score.distance_m / 1000.0 - perimeter).abs() < 0.1,
            "{score:?}"
        );
        assert!((score.points - score.distance_m / 1000.0 * 1.6).abs() < 1e-6);
        assert!(score.closing_m.unwrap() < 100.0);
        assert_eq!(score.route.len(), 5);
        assert!(score.route.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn a_triangle_left_open_by_a_tenth_loses_the_gap_and_the_closed_bonus() {
        // The same triangle, landing 4 km short of the start: about 11% of
        // the perimeter.
        let fixes = track_through(&[(0.0, 0.0), (12.0, 0.0), (7.0, 12.0), (2.0, 3.46)]);
        let score = score(&fixes).unwrap();

        assert_eq!(score.kind, RouteKind::FaiTriangle);
        assert!(!score.closed);
        let closing = score.closing_m.unwrap();
        assert!((closing / 1000.0 - 4.0).abs() < 0.15, "{closing}");
        assert!((score.points - score.distance_m / 1000.0 * 1.4).abs() < 1e-6);
        let perimeter = 12.0 + 5.0_f64.hypot(12.0) + 7.0_f64.hypot(12.0);
        assert!(
            (score.distance_m / 1000.0 - (perimeter - 4.0)).abs() < 0.2,
            "{score:?}"
        );
    }

    #[test]
    fn a_long_thin_triangle_is_a_free_triangle() {
        // Out 15 km, a 3 km side step, and back: the short leg is under
        // 28% of the perimeter.
        let fixes = track_through(&[(0.0, 0.0), (15.0, 0.0), (15.0, 3.0), (0.0, 0.0)]);
        let score = score(&fixes).unwrap();

        assert_eq!(score.kind, RouteKind::FreeTriangle);
        assert!(score.closed);
        assert!((score.points - score.distance_m / 1000.0 * 1.4).abs() < 1e-6);
    }

    #[test]
    fn a_triangle_left_wide_open_is_scored_as_a_free_flight() {
        // Two legs of a triangle and a third that stops a quarter of the
        // way home, more than 10 km from the start: over 20% of any
        // triangle this track could close.
        let fixes = track_through(&[(0.0, 0.0), (12.0, 0.0), (7.0, 12.0), (5.25, 9.0)]);
        let score = score(&fixes).unwrap();
        assert_eq!(score.kind, RouteKind::FreeFlight);
        let legs = 12.0 + 5.0_f64.hypot(12.0) + 1.75_f64.hypot(3.0);
        assert!((score.distance_m / 1000.0 - legs).abs() < 0.1, "{score:?}");
    }

    #[test]
    fn the_made_up_flight_is_scored_and_its_route_is_in_flight_order() {
        let flight = synthetic_flight();
        let score = score(&flight.fixes).unwrap();

        assert!(score.points > 10.0, "{score:?}");
        assert!(score.route.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(*score.route.last().unwrap() < flight.fixes.len());
    }

    #[test]
    fn too_short_a_track_has_no_score() {
        assert_eq!(score(&[]), None);
        assert_eq!(score(&track_through(&[(0.0, 0.0)])), None);
    }
}
