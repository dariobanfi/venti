//! Wind from the way ground speed changes around a circle.

use std::collections::VecDeque;

/// A wind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wind {
    /// The direction the wind comes from, degrees clockwise from north.
    pub from_deg: f64,
    /// Its speed in metres per second.
    pub speed_ms: f64,
}

/// How much flying the estimate looks back on, seconds: a little more than
/// one circle in a thermal.
const WINDOW_S: f64 = 40.0;

/// The directions of travel are sorted into this many sectors; the estimate
/// needs samples in nearly all of them, which means nearly a full circle.
const SECTORS: usize = 12;
const SECTORS_NEEDED: usize = 10;

/// The glider is turning while its direction of travel changes at least
/// this fast, in degrees per second. A circle in a thermal turns at 12 to
/// 24; straight flight wanders by one or two.
const TURNING_DEG_PER_S: f64 = 4.0;

/// This many samples in a row below that rate end a turn.
const STRAIGHT_SAMPLES_END_TURN: u32 = 3;

/// Estimates the wind while the glider circles.
///
/// A glider flying a circle at constant airspeed moves over the ground
/// faster with the wind behind it and slower against it. Plotted as arrows
/// from one point, its ground velocities lie on a circle whose radius is the
/// airspeed and whose centre, measured from that point, is the wind. The
/// estimator fits that circle to the last 40 seconds of ground velocity.
/// It needs no compass and no airspeed sensor.
///
/// Straight flight tells it nothing, and fitted together with part of a
/// circle it would give a wrong wind. So only one unbroken stretch of
/// turning is ever fitted, and the estimate stays as it was from the moment
/// the glider straightens out until it has nearly completed its next circle.
#[derive(Debug, Clone, Default)]
pub struct WindEstimator {
    samples: VecDeque<(f64, f64, f64)>,
    /// The time span of the current stretch of turning: from its first
    /// sample to its latest. `None` in straight flight.
    turn: Option<(f64, f64)>,
    straight_samples: u32,
    estimate: Option<Wind>,
}

impl WindEstimator {
    /// An estimator that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes one ground velocity: its east and north parts in metres per
    /// second, and when it was measured. Returns the wind if this sample
    /// gave a new measurement of it: `None` in straight flight, where the
    /// estimate stays as it was.
    pub fn add(&mut self, time_s: f64, east_ms: f64, north_ms: f64) -> Option<Wind> {
        if let Some(&(last_time, last_east, last_north)) = self.samples.back() {
            let course = |east: f64, north: f64| east.atan2(north).to_degrees();
            let turned = (course(east_ms, north_ms) - course(last_east, last_north) + 540.0)
                .rem_euclid(360.0)
                - 180.0;
            let seconds = time_s - last_time;
            let moving = east_ms.hypot(north_ms) > 1.0 && last_east.hypot(last_north) > 1.0;
            if moving && seconds > 0.0 && turned.abs() / seconds >= TURNING_DEG_PER_S {
                self.turn = Some((self.turn.map_or(last_time, |(since, _)| since), time_s));
                self.straight_samples = 0;
            } else {
                self.straight_samples += 1;
                if self.straight_samples >= STRAIGHT_SAMPLES_END_TURN {
                    self.turn = None;
                }
            }
        }
        self.samples.push_back((time_s, east_ms, north_ms));
        while self
            .samples
            .front()
            .is_some_and(|&(time, _, _)| time < time_s - WINDOW_S)
        {
            self.samples.pop_front();
        }
        let measured = self.fit();
        if let Some(wind) = measured {
            self.estimate = Some(wind);
        }
        measured
    }

    /// The latest estimate, if the glider has circled yet.
    pub fn wind(&self) -> Option<Wind> {
        self.estimate
    }

    fn fit(&self) -> Option<Wind> {
        // Only the current stretch of turning, up to its latest turning
        // sample: nothing from before it began or after it faltered.
        let (since, until) = self.turn?;
        let samples: Vec<(f64, f64, f64)> = self
            .samples
            .iter()
            .copied()
            .filter(|&(time, _, _)| time >= since && time <= until)
            .collect();

        let mut covered = [false; SECTORS];
        for &(_, east, north) in &samples {
            if east.hypot(north) > 1.0 {
                let course = east.atan2(north).to_degrees().rem_euclid(360.0);
                covered[(course / (360.0 / SECTORS as f64)) as usize % SECTORS] = true;
            }
        }
        if covered.iter().filter(|&&sector| sector).count() < SECTORS_NEEDED {
            return None;
        }

        // Least-squares circle through the velocity points: find D, E, F
        // minimising the sum of (x² + y² + Dx + Ey + F)². That is linear in
        // D, E and F, so it reduces to three equations.
        let n = samples.len() as f64;
        let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let (mut sxz, mut syz, mut sz) = (0.0, 0.0, 0.0);
        for &(_, x, y) in &samples {
            let z = x * x + y * y;
            sx += x;
            sy += y;
            sxx += x * x;
            syy += y * y;
            sxy += x * y;
            sxz += x * z;
            syz += y * z;
            sz += z;
        }
        let [d, e, f] = solve(
            [[sxx, sxy, sx], [sxy, syy, sy], [sx, sy, n]],
            [-sxz, -syz, -sz],
        )?;
        let (centre_east, centre_north) = (-d / 2.0, -e / 2.0);
        let airspeed = (centre_east * centre_east + centre_north * centre_north - f).sqrt();

        // A paraglider flies between about 6 and 20 m/s. A fit outside that,
        // or one the points do not lie close to, was not a steady circle.
        if !(4.0..=25.0).contains(&airspeed) {
            return None;
        }
        let misfit = (samples
            .iter()
            .map(|&(_, x, y)| ((x - centre_east).hypot(y - centre_north) - airspeed).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
        if misfit > 2.0 {
            return None;
        }

        Some(Wind {
            // The centre is where the air is going; wind is named for where
            // it comes from.
            from_deg: (-centre_east)
                .atan2(-centre_north)
                .to_degrees()
                .rem_euclid(360.0),
            speed_ms: centre_east.hypot(centre_north),
        })
    }
}

/// Solves three linear equations by Gaussian elimination. `None` if they
/// have no single solution.
fn solve(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    for column in 0..3 {
        let pivot =
            (column..3).max_by(|&i, &j| a[i][column].abs().total_cmp(&a[j][column].abs()))?;
        if a[pivot][column].abs() < 1e-9 {
            return None;
        }
        a.swap(column, pivot);
        b.swap(column, pivot);
        for row in column + 1..3 {
            let pivot_row = a[column];
            let factor = a[row][column] / pivot_row[column];
            for (value, pivot) in a[row].iter_mut().zip(pivot_row).skip(column) {
                *value -= factor * pivot;
            }
            b[row] -= factor * b[column];
        }
    }
    let mut x = [0.0; 3];
    for row in (0..3).rev() {
        let known: f64 = (row + 1..3).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - known) / a[row][row];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use super::*;

    /// Feeds circles flown at `airspeed` with the wind blowing toward
    /// (`wind_east`, `wind_north`), one sample a second.
    fn circle(estimator: &mut WindEstimator, seconds: u32, airspeed: f64, wind: (f64, f64)) {
        for second in 0..seconds {
            let heading = TAU * f64::from(second) / 28.0;
            estimator.add(
                f64::from(second),
                airspeed * heading.sin() + wind.0,
                airspeed * heading.cos() + wind.1,
            );
        }
    }

    #[test]
    fn finds_a_westerly_from_one_circle() {
        let mut estimator = WindEstimator::new();
        circle(&mut estimator, 30, 10.0, (3.0, 0.0));
        let wind = estimator.wind().unwrap();
        assert!((wind.from_deg - 270.0).abs() < 1.0, "{wind:?}");
        assert!((wind.speed_ms - 3.0).abs() < 0.05, "{wind:?}");
    }

    #[test]
    fn finds_a_north_easterly() {
        let mut estimator = WindEstimator::new();
        // Blowing toward the south-west at 5 m/s.
        let part = 5.0 / 2.0_f64.sqrt();
        circle(&mut estimator, 60, 9.0, (-part, -part));
        let wind = estimator.wind().unwrap();
        assert!((wind.from_deg - 45.0).abs() < 1.0, "{wind:?}");
        assert!((wind.speed_ms - 5.0).abs() < 0.05, "{wind:?}");
    }

    #[test]
    fn says_nothing_in_straight_flight_or_a_half_turn() {
        let mut estimator = WindEstimator::new();
        for second in 0..120 {
            estimator.add(f64::from(second), 8.0, 6.0);
        }
        assert_eq!(estimator.wind(), None);

        let mut estimator = WindEstimator::new();
        circle(&mut estimator, 14, 10.0, (3.0, 0.0));
        assert_eq!(estimator.wind(), None);
    }

    #[test]
    fn keeps_its_estimate_on_the_glide_after_the_thermal() {
        let mut estimator = WindEstimator::new();
        circle(&mut estimator, 56, 10.0, (0.0, 4.0));
        let in_thermal = estimator.wind().unwrap();
        for second in 56..300 {
            estimator.add(f64::from(second), 11.0, 4.0);
        }
        assert!((in_thermal.from_deg - 180.0).abs() < 1.0);
        // Only the sample on which the glider rolled out still counts as
        // turning; after that nothing is fitted and the estimate holds.
        let on_glide = estimator.wind().unwrap();
        assert!(
            (on_glide.from_deg - in_thermal.from_deg).abs() < 2.0,
            "{on_glide:?}"
        );
        assert!(
            (on_glide.speed_ms - in_thermal.speed_ms).abs() < 0.1,
            "{on_glide:?}"
        );
    }

    #[test]
    fn rejects_turning_that_is_not_a_steady_circle() {
        let mut estimator = WindEstimator::new();
        // Speeds jumping between 2 and 20 m/s while the direction sweeps.
        for second in 0..40 {
            let heading = TAU * f64::from(second) / 28.0;
            let speed = if second % 2 == 0 { 2.0 } else { 20.0 };
            estimator.add(
                f64::from(second),
                speed * heading.sin(),
                speed * heading.cos(),
            );
        }
        assert_eq!(estimator.wind(), None);
    }
}
