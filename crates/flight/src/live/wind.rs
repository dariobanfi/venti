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

/// The directions the glider pointed in are sorted into this many sectors;
/// the estimate needs samples in nearly all of them, which means nearly a
/// full circle.
const SECTORS: usize = 12;
const SECTORS_NEEDED: usize = 10;

/// The glider is turning while its velocity over the ground changes at
/// least this fast, in metres per second each second. A glider on a circle
/// is pulled toward the middle by its speed through the air times its rate
/// of turn, whatever the wind: 0.7 is a turn of four degrees a second at
/// ten metres per second. A circle in a thermal gives 2 to 4; straight
/// flight a few tenths.
///
/// The direction of travel over the ground would not do. With the wind
/// from behind it hardly changes, and in a wind stronger than the glider
/// is fast the glider is blown backwards on the upwind half of its circle
/// and its track never points into the wind at all.
const TURNING_MS2: f64 = 0.7;

/// The change of velocity is taken over this long, seconds: each velocity
/// is held against the one from four seconds before. From one second to
/// the next a velocity worked out from a recorder's positions, which are
/// rounded to two metres, changes by more than a metre per second with the
/// glider flying dead straight, and every second of a glide would count as
/// turning. Over four seconds that noise is a quarter as large, and a
/// circle's change is as large as ever. A turn that goes all the way
/// around in four seconds, which no glider flies in a thermal, is not seen.
///
/// What is found is known of the middle of those four seconds: a turn is
/// dated from two seconds before it shows to two seconds before it stops
/// showing, and the fit runs two seconds behind the glider.
const TURN_BASELINE_S: f64 = 4.0;

/// This many samples in a row below that rate end a turn.
const STRAIGHT_SAMPLES_END_TURN: u32 = 3;

/// The points may lie this far from the fitted circle, root mean square in
/// metres per second, and no further than [`MISFIT_SHARE`] of its radius.
const MISFIT_LIMIT_MS: f64 = 2.0;

/// See [`MISFIT_LIMIT_MS`]. A cloud of noise around one velocity fits a
/// circle too, but badly for its size.
const MISFIT_SHARE: f64 = 0.25;

/// One measurement of the wind: the fit to one stretch of circling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindFix {
    /// The wind.
    pub wind: Wind,
    /// The speed through the air the glider circled at, metres per second.
    pub airspeed_ms: f64,
    /// How far to trust it, above 0 and up to 1: how close the velocities
    /// lay to the circle, times the share of the circle they covered. For
    /// weighing one measurement against another, not a probability.
    pub quality: f64,
}

/// Estimates the wind while the glider circles.
///
/// A glider flying a circle at constant airspeed moves over the ground
/// faster with the wind behind it and slower against it. Plotted as arrows
/// from one point, its ground velocities lie on a circle whose radius is the
/// airspeed and whose centre, measured from that point, is the wind. The
/// estimator fits that circle to the last 40 seconds of ground velocity.
/// It needs no compass and no airspeed sensor, and it works the same in a
/// wind stronger than the glider is fast, where the circle of velocities
/// no longer goes around zero.
///
/// Straight flight tells it nothing, and fitted together with part of a
/// circle it would give a wrong wind. So only one unbroken stretch of
/// turning is ever fitted, and the estimate stays as it was from the moment
/// the glider straightens out until it has nearly completed its next circle.
///
/// The method is the one gliding computers have long used: Cumulus and
/// XCSoar fit a cosine to the ground speed around each circle, which is the
/// same picture seen along one axis. The code here is the project's own.
#[derive(Debug, Clone, Default)]
pub struct WindEstimator {
    samples: VecDeque<(f64, f64, f64)>,
    /// The time span of the current stretch of turning: from its first
    /// sample to its latest. `None` in straight flight.
    turn: Option<(f64, f64)>,
    straight_samples: u32,
    estimate: Option<WindFix>,
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
    pub fn add(&mut self, time_s: f64, east_ms: f64, north_ms: f64) -> Option<WindFix> {
        // The latest sample old enough to measure a change against.
        let before = self
            .samples
            .iter()
            .rev()
            .find(|&&(time, _, _)| time <= time_s - TURN_BASELINE_S)
            .copied();
        if let Some((last_time, last_east, last_north)) = before {
            let seconds = time_s - last_time;
            let change = (east_ms - last_east).hypot(north_ms - last_north);
            if change / seconds >= TURNING_MS2 {
                // Known of the middle of the stretch looked at.
                let middle = time_s - seconds / 2.0;
                self.turn = Some((self.turn.map_or(middle, |(since, _)| since), middle));
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
        if let Some(fix) = measured {
            self.estimate = Some(fix);
        }
        measured
    }

    /// The latest estimate, if the glider has circled yet.
    pub fn wind(&self) -> Option<Wind> {
        self.estimate.map(|fix| fix.wind)
    }

    /// The latest measurement with what is known about it.
    pub fn latest(&self) -> Option<WindFix> {
        self.estimate
    }

    fn fit(&self) -> Option<WindFix> {
        // Only the current stretch of turning, up to its latest turning
        // sample: nothing from before it began or after it faltered.
        let (since, until) = self.turn?;
        let samples: Vec<(f64, f64, f64)> = self
            .samples
            .iter()
            .copied()
            .filter(|&(time, _, _)| time >= since && time <= until)
            .collect();

        // A full circle has at least one sample in nearly every sector.
        if samples.len() < SECTORS_NEEDED {
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
        let allowed = MISFIT_LIMIT_MS.min(MISFIT_SHARE * airspeed);
        if misfit > allowed {
            return None;
        }

        // Seen from the centre, each velocity lies in the direction the
        // glider pointed. Around a circle it points everywhere once, however
        // strong the wind; its track over the ground does not.
        let mut covered = [false; SECTORS];
        for &(_, east, north) in &samples {
            let heading = (east - centre_east)
                .atan2(north - centre_north)
                .to_degrees()
                .rem_euclid(360.0);
            covered[(heading / (360.0 / SECTORS as f64)) as usize % SECTORS] = true;
        }
        let sectors = covered.iter().filter(|&&sector| sector).count();
        if sectors < SECTORS_NEEDED {
            return None;
        }

        Some(WindFix {
            wind: Wind {
                // The centre is where the air is going; wind is named for
                // where it comes from.
                from_deg: (-centre_east)
                    .atan2(-centre_north)
                    .to_degrees()
                    .rem_euclid(360.0),
                speed_ms: centre_east.hypot(centre_north),
            },
            airspeed_ms: airspeed,
            quality: ((1.0 - misfit / allowed) * sectors as f64 / SECTORS as f64).max(0.01),
        })
    }
}

/// Solves three linear equations by Gaussian elimination. `None` if they
/// have no single solution.
pub(super) fn solve(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
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
    fn finds_a_wind_stronger_than_the_glider_is_fast() {
        // Twelve metres per second from the north against a glider flying
        // ten: over the ground it only ever travels between south-east and
        // south-west, and a rule that waits for its track to come around
        // waits for ever.
        let mut estimator = WindEstimator::new();
        circle(&mut estimator, 40, 10.0, (0.0, -12.0));
        let fix = estimator.latest().unwrap();
        assert!(
            fix.wind.from_deg < 1.0 || fix.wind.from_deg > 359.0,
            "{fix:?}"
        );
        assert!((fix.wind.speed_ms - 12.0).abs() < 0.05, "{fix:?}");
        assert!((fix.airspeed_ms - 10.0).abs() < 0.05, "{fix:?}");
        assert!(fix.quality > 0.9, "{fix:?}");
    }

    #[test]
    fn finds_a_wind_nearly_as_strong_as_the_glider_is_fast() {
        // Here the track does come around, but in two samples: it sweeps
        // through the upwind half faster than a receiver reports.
        let mut estimator = WindEstimator::new();
        circle(&mut estimator, 30, 10.0, (9.5, 0.0));
        let wind = estimator.wind().unwrap();
        assert!((wind.from_deg - 270.0).abs() < 1.0, "{wind:?}");
        assert!((wind.speed_ms - 9.5).abs() < 0.05, "{wind:?}");
    }

    /// Numbers between -1 and 1 that look random and are the same on every
    /// run.
    fn noise(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((*seed >> 33) as f64 / f64::from(1_u32 << 31)) * 2.0 - 1.0
    }

    #[test]
    fn a_rough_circle_is_trusted_less_than_a_clean_one() {
        let mut clean = WindEstimator::new();
        circle(&mut clean, 40, 10.0, (3.0, 0.0));

        let mut rough = WindEstimator::new();
        let mut seed = 7;
        for second in 0..40 {
            let heading = TAU * f64::from(second) / 28.0;
            rough.add(
                f64::from(second),
                10.0 * heading.sin() + 3.0 + noise(&mut seed),
                10.0 * heading.cos() + noise(&mut seed),
            );
        }
        let (clean, rough) = (clean.latest().unwrap(), rough.latest().unwrap());
        assert!(clean.quality > 0.95, "{clean:?}");
        assert!(rough.quality < 0.8 && rough.quality > 0.3, "{rough:?}");
        assert!((rough.wind.from_deg - 270.0).abs() < 12.0, "{rough:?}");
        assert!((rough.wind.speed_ms - 3.0).abs() < 0.6, "{rough:?}");
    }

    #[test]
    fn says_nothing_about_noise_around_straight_flight() {
        // A receiver whose speed jumps about by three metres per second
        // looks like turning all the time. The cloud of its velocities has
        // a middle and a size, but it is no circle.
        let mut estimator = WindEstimator::new();
        let mut seed = 99;
        for second in 0..300 {
            estimator.add(
                f64::from(second),
                8.0 + 3.0 * noise(&mut seed),
                6.0 + 3.0 * noise(&mut seed),
            );
        }
        assert_eq!(estimator.wind(), None);
    }

    #[test]
    fn rough_velocities_on_the_glide_are_not_taken_for_more_circling() {
        // Velocities worked out from a recorder's positions, which are
        // rounded to two metres: off by up to a metre per second in each
        // part, every second, in the circles and on the glide alike.
        let mut estimator = WindEstimator::new();
        let mut seed = 2026;
        for second in 0..84 {
            let heading = TAU * f64::from(second) / 28.0;
            estimator.add(
                f64::from(second),
                9.5 * heading.sin() + 3.0 + noise(&mut seed),
                9.5 * heading.cos() + noise(&mut seed),
            );
        }
        let in_thermal = estimator.wind().unwrap();
        assert!((in_thermal.from_deg - 270.0).abs() < 8.0, "{in_thermal:?}");
        assert!((in_thermal.speed_ms - 3.0).abs() < 0.5, "{in_thermal:?}");

        // The glide: faster through the air than the circles were, as a
        // glide is. Fitted together with them it would give another wind.
        let mut measured_late = 0;
        for second in 84..400 {
            let fix = estimator.add(
                f64::from(second),
                -9.5 + 3.0 + noise(&mut seed),
                9.0 + noise(&mut seed),
            );
            if second > 90 && fix.is_some() {
                measured_late += 1;
            }
            let wind = estimator.wind().unwrap();
            assert!(
                (wind.from_deg - 270.0).abs() < 10.0 && (wind.speed_ms - 3.0).abs() < 0.6,
                "second {second}: {wind:?}"
            );
        }
        assert_eq!(measured_late, 0);
    }

    #[test]
    fn a_receiver_that_reports_ten_times_a_second_changes_nothing() {
        let mut estimator = WindEstimator::new();
        for tenth in 0..400 {
            let time = f64::from(tenth) / 10.0;
            let heading = TAU * time / 28.0;
            estimator.add(time, 10.0 * heading.sin() + 3.0, 10.0 * heading.cos());
        }
        let wind = estimator.wind().unwrap();
        assert!((wind.from_deg - 270.0).abs() < 1.0, "{wind:?}");
        assert!((wind.speed_ms - 3.0).abs() < 0.05, "{wind:?}");

        // And straight flight at that rate, with a little noise, is not
        // taken for turning: the estimate holds.
        let mut seed = 5;
        let mut measured = 0;
        for tenth in 400..1400 {
            let time = f64::from(tenth) / 10.0;
            let fix = estimator.add(
                time,
                11.0 + 0.05 * noise(&mut seed),
                4.0 + 0.05 * noise(&mut seed),
            );
            if time > 50.0 && fix.is_some() {
                measured += 1;
            }
        }
        assert_eq!(measured, 0);
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
