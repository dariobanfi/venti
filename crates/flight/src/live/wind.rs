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

/// A velocity that lies further than this from the fitted circle, metres
/// per second, or further than twice what they all do on average if that
/// is more, is left out of the second fit.
const OUTLIER_MS: f64 = 1.5;

/// A fit needs at least this many velocities. A recorder that writes a
/// position every four or five seconds leaves no more than this to a
/// circle.
const SAMPLES_NEEDED: usize = 6;

/// From one velocity to the next the glider is taken to have turned
/// through everything in between, as long as that is no more than this
/// many degrees: a third of a circle between two positions is a recorder
/// that writes one every five seconds in a tight thermal. Beyond it,
/// which way round the glider went cannot be told.
const STEP_AT_MOST_DEG: f64 = 120.0;

/// A circle is flown one way round. Of all the turning from one velocity
/// to the next, seen from the fitted centre, at least this share must be
/// left when left and right are set against each other. Around a real
/// circle nearly all of it is. A glider flying straight through rough
/// air, or pulling up and pushing over, leaves a cloud of velocities that
/// fits a small circle well enough and whose directions go to and fro:
/// that gave recorded flights a wind of 127 km/h.
const ONE_WAY_SHARE: f64 = 0.8;

/// A wind this many times the speed the glider circled at is refused:
/// nobody circles in a wind twice as fast as the glider. A second net
/// under [`ONE_WAY_SHARE`].
const WIND_AT_MOST_AIRSPEEDS: f64 = 2.0;

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

    /// The moment up to which the glider is known to be turning, in the
    /// time of the samples: two seconds behind the newest while a turn
    /// goes on. `None` in straight flight.
    pub fn turning_until(&self) -> Option<f64> {
        self.turn.map(|(_, until)| until)
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

        if samples.len() < SAMPLES_NEEDED {
            return None;
        }

        // The circle through all of them; then once more without the
        // velocities that lie well off it. Those are the seconds of rolling
        // in and out, where the glider flies faster than in its circles,
        // and a gust or two. The fit that is made as the glider leaves a
        // thermal is the one that stands for the whole glide after it, and
        // two seconds of glide in it moved the wind by a tenth.
        let mut samples = samples;
        let mut circle = circle_through(&samples)?;
        let off = |circle: (f64, f64, f64), &(_, x, y): &(f64, f64, f64)| {
            ((x - circle.0).hypot(y - circle.1) - circle.2).abs()
        };
        let spread = |circle: (f64, f64, f64), samples: &[(f64, f64, f64)]| {
            (samples
                .iter()
                .map(|sample| off(circle, sample).powi(2))
                .sum::<f64>()
                / samples.len() as f64)
                .sqrt()
        };
        let far = OUTLIER_MS.max(2.0 * spread(circle, &samples));
        let near: Vec<(f64, f64, f64)> = samples
            .iter()
            .copied()
            .filter(|sample| off(circle, sample) <= far)
            .collect();
        if near.len() < samples.len() && near.len() >= SAMPLES_NEEDED {
            circle = circle_through(&near)?;
            samples = near;
        }
        let (centre_east, centre_north, airspeed) = circle;

        // A paraglider flies between about 6 and 20 m/s. A fit outside that,
        // or one the points do not lie close to, was not a steady circle.
        if !(4.0..=25.0).contains(&airspeed) {
            return None;
        }
        let misfit = spread(circle, &samples);
        let allowed = MISFIT_LIMIT_MS.min(MISFIT_SHARE * airspeed);
        if misfit > allowed {
            return None;
        }

        let wind_speed = centre_east.hypot(centre_north);
        if wind_speed > WIND_AT_MOST_AIRSPEEDS * airspeed {
            return None;
        }

        // Seen from the centre, each velocity lies in the direction the
        // glider pointed. Around a circle it points everywhere once, however
        // strong the wind; its track over the ground does not. From one
        // velocity to the next it turns a little further the same way, and
        // it has pointed at everything in between.
        let sector_deg = 360.0 / SECTORS as f64;
        let sector = |heading: f64| (heading.rem_euclid(360.0) / sector_deg) as usize % SECTORS;
        let headings: Vec<f64> = samples
            .iter()
            .map(|&(_, east, north)| {
                (east - centre_east)
                    .atan2(north - centre_north)
                    .to_degrees()
            })
            .collect();
        let mut covered = [false; SECTORS];
        let (mut turned, mut wandered) = (0.0, 0.0);
        for (index, &heading) in headings.iter().enumerate() {
            covered[sector(heading)] = true;
            let Some(&next) = headings.get(index + 1) else {
                break;
            };
            let step = (next - heading + 540.0).rem_euclid(360.0) - 180.0;
            turned += step;
            wandered += step.abs();
            if step.abs() <= STEP_AT_MOST_DEG {
                let parts = (step.abs() / (sector_deg / 2.0)).ceil().max(1.0);
                for part in 1..parts as usize {
                    covered[sector(heading + step * part as f64 / parts)] = true;
                }
            }
        }
        if turned.abs() < ONE_WAY_SHARE * wandered {
            return None;
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
                speed_ms: wind_speed,
            },
            airspeed_ms: airspeed,
            quality: ((1.0 - misfit / allowed) * sectors as f64 / SECTORS as f64).max(0.01),
        })
    }
}

/// Where a wind was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindSource {
    /// From nearly a whole circle: [`WindEstimator`].
    Circling,
    /// From a turn of part of a circle on a glide: [`GlideWindEstimator`].
    Glide,
}

/// The glide's estimate looks back this many seconds: a minute, in which
/// the wind and the speed through the air are taken to have stayed the
/// same.
const GLIDE_WINDOW_S: f64 = 60.0;

/// The least arc the glide's estimate asks for, degrees: seen from the
/// fitted centre, the directions the glider pointed in must span a third
/// of a circle. Over less, the circle through the velocities is hardly
/// bent, and its centre, which is the wind, slides along the line through
/// its middle with every bit of noise: on made-up turns of 90° with half a
/// metre per second of noise the wind was off by three metres per second
/// and more, on 120° by one. To verify on recorded flights.
pub const GLIDE_ARC_LEAST_DEG: f64 = 120.0;

/// The arc is counted in sectors of this many degrees.
const GLIDE_SECTOR_DEG: f64 = 15.0;

/// The glide's fit takes one velocity a second at the most, and needs this
/// many.
const GLIDE_SAMPLES_NEEDED: usize = 10;

/// The speed through the air a glide's fit may give, metres per second:
/// 22 to 72 km/h. Outside it the velocities were no paraglider's turn.
const GLIDE_AIRSPEED_MS: std::ops::RangeInclusive<f64> = 6.0..=20.0;

/// The glide's velocities may lie this far from the fitted circle, root
/// mean square in metres per second, and no further than
/// [`GLIDE_MISFIT_SHARE`] of its radius: half of what a whole circle is
/// allowed, since an arc has less to hold a bad fit against.
const GLIDE_MISFIT_LIMIT_MS: f64 = 1.0;

/// See [`GLIDE_MISFIT_LIMIT_MS`].
const GLIDE_MISFIT_SHARE: f64 = 0.12;

/// Seen from the fitted centre, the direction the glider points in may
/// move this many degrees a second on average, to and fro counted
/// together. A turn on a glide is two to six; velocities that ring a
/// point in no order, which fit a circle well and are none, jump by tens.
const GLIDE_TURN_AT_MOST_DEG_S: f64 = 12.0;

/// Estimates the wind on a glide, from a turn of part of a circle.
///
/// The picture is the one of [`WindEstimator`]: at one speed through the
/// air, the velocities over the ground lie on a circle around the wind.
/// A glider that changes its heading by a third of a circle or more
/// within a minute, around a spur, onto a new leg or in a wide S, has
/// drawn enough of that circle to fit it. It says nothing from a straight
/// line, whatever the ground speed does on it: a line bends around no
/// centre.
///
/// What it takes for granted, and cannot check: that the glider flew one
/// speed through the air through the whole minute, and that the wind was
/// one wind. A pilot who turns and steps on the speed bar at once draws
/// an arc of another circle, and the wind fitted is wrong by about the
/// change of speed. The estimate is therefore trusted less than a
/// circle's ([`WindFix::quality`] is the fit times the share of a circle
/// the arc covers), and the flight computer lets a circle's wind stand
/// against it until that has aged.
#[derive(Debug, Clone, Default)]
pub struct GlideWindEstimator {
    samples: VecDeque<(f64, f64, f64)>,
}

impl GlideWindEstimator {
    /// An estimator that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets what it has seen: for the moment a circle has given the
    /// wind, whose velocities are not a glide's.
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Takes one ground velocity, east and north in metres per second,
    /// and when it was measured. Returns a wind if the last minute holds
    /// an arc that gives one.
    pub fn add(&mut self, time_s: f64, east_ms: f64, north_ms: f64) -> Option<WindFix> {
        // One a second at the most, so that a receiver that reports ten
        // times a second is weighed like any other.
        if self
            .samples
            .back()
            .is_some_and(|&(last, _, _)| time_s - last < 0.9)
        {
            return None;
        }
        self.samples.push_back((time_s, east_ms, north_ms));
        while self
            .samples
            .front()
            .is_some_and(|&(time, _, _)| time < time_s - GLIDE_WINDOW_S)
        {
            self.samples.pop_front();
        }
        self.fit()
    }

    fn fit(&self) -> Option<WindFix> {
        if self.samples.len() < GLIDE_SAMPLES_NEEDED {
            return None;
        }
        let mut samples: Vec<(f64, f64, f64)> = self.samples.iter().copied().collect();
        // Cheap and first: velocities that all lie within a few metres per
        // second of each other span no arc of any circle a glider flies.
        let chord_needed =
            2.0 * GLIDE_AIRSPEED_MS.start() * (GLIDE_ARC_LEAST_DEG / 2.0).to_radians().sin();
        let (first_east, first_north) = (samples[0].1, samples[0].2);
        let widest = samples
            .iter()
            .map(|&(_, east, north)| (east - first_east).hypot(north - first_north))
            .fold(0.0, f64::max);
        if widest < chord_needed / 2.0 {
            return None;
        }

        let mut circle = circle_through(&samples)?;
        let off = |circle: (f64, f64, f64), &(_, x, y): &(f64, f64, f64)| {
            ((x - circle.0).hypot(y - circle.1) - circle.2).abs()
        };
        let spread = |circle: (f64, f64, f64), samples: &[(f64, f64, f64)]| {
            (samples
                .iter()
                .map(|sample| off(circle, sample).powi(2))
                .sum::<f64>()
                / samples.len() as f64)
                .sqrt()
        };
        let far = OUTLIER_MS.max(2.0 * spread(circle, &samples));
        let near: Vec<(f64, f64, f64)> = samples
            .iter()
            .copied()
            .filter(|sample| off(circle, sample) <= far)
            .collect();
        if near.len() < samples.len() && near.len() >= GLIDE_SAMPLES_NEEDED {
            circle = circle_through(&near)?;
            samples = near;
        }
        let (centre_east, centre_north, airspeed) = circle;
        if !GLIDE_AIRSPEED_MS.contains(&airspeed) {
            return None;
        }
        let misfit = spread(circle, &samples);
        let allowed = GLIDE_MISFIT_LIMIT_MS.min(GLIDE_MISFIT_SHARE * airspeed);
        if misfit > allowed {
            return None;
        }
        let wind_speed = centre_east.hypot(centre_north);
        if wind_speed > WIND_AT_MOST_AIRSPEEDS * airspeed {
            return None;
        }

        // The arc: the directions the glider pointed in, seen from the
        // centre, with what lies between one and the next.
        let sectors = (360.0 / GLIDE_SECTOR_DEG) as usize;
        let sector =
            |heading: f64| (heading.rem_euclid(360.0) / GLIDE_SECTOR_DEG) as usize % sectors;
        let headings: Vec<f64> = samples
            .iter()
            .map(|&(_, east, north)| {
                (east - centre_east)
                    .atan2(north - centre_north)
                    .to_degrees()
            })
            .collect();
        let mut covered = vec![false; sectors];
        let mut wandered = 0.0;
        for (index, &heading) in headings.iter().enumerate() {
            covered[sector(heading)] = true;
            let Some(&next) = headings.get(index + 1) else {
                break;
            };
            let step = (next - heading + 540.0).rem_euclid(360.0) - 180.0;
            wandered += step.abs();
            if step.abs() <= STEP_AT_MOST_DEG {
                let parts = (step.abs() / (GLIDE_SECTOR_DEG / 2.0)).ceil().max(1.0);
                for part in 1..parts as usize {
                    covered[sector(heading + step * part as f64 / parts)] = true;
                }
            }
        }
        let seconds = samples.last()?.0 - samples.first()?.0;
        if seconds <= 0.0 || wandered / seconds > GLIDE_TURN_AT_MOST_DEG_S {
            return None;
        }
        let arc_deg = covered.iter().filter(|&&sector| sector).count() as f64 * GLIDE_SECTOR_DEG;
        if arc_deg < GLIDE_ARC_LEAST_DEG {
            return None;
        }

        Some(WindFix {
            wind: Wind {
                from_deg: (-centre_east)
                    .atan2(-centre_north)
                    .to_degrees()
                    .rem_euclid(360.0),
                speed_ms: wind_speed,
            },
            airspeed_ms: airspeed,
            quality: ((1.0 - misfit / allowed) * arc_deg / 360.0).max(0.01),
        })
    }
}

/// The circle that fits a set of velocities best: its centre, east and
/// north, and its radius. Least squares, by finding D, E and F that
/// minimise the sum of (x² + y² + Dx + Ey + F)², which is linear in the
/// three.
fn circle_through(samples: &[(f64, f64, f64)]) -> Option<(f64, f64, f64)> {
    let n = samples.len() as f64;
    let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut sxz, mut syz, mut sz) = (0.0, 0.0, 0.0);
    for &(_, x, y) in samples {
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
    let radius = (centre_east * centre_east + centre_north * centre_north - f).sqrt();
    radius
        .is_finite()
        .then_some((centre_east, centre_north, radius))
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
        // Two seconds of the glide are in the last fits. Here the glide is
        // one metre per second faster than the circles, which is too near
        // the circle to be told from it, and moves the wind by two degrees
        // at most. After them nothing is fitted and the estimate holds.
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
    fn the_seconds_of_rolling_out_do_not_move_the_wind() {
        // Circles at 9.5 m/s in a westerly of 3, then a glide north-west at
        // 13.5 m/s through the air: well off the circle of velocities.
        let mut estimator = WindEstimator::new();
        circle(&mut estimator, 84, 9.5, (3.0, 0.0));
        for second in 84..200 {
            estimator.add(f64::from(second), -9.5 + 3.0, 9.5);
        }
        let on_glide = estimator.wind().unwrap();
        assert!((on_glide.from_deg - 270.0).abs() < 0.5, "{on_glide:?}");
        assert!((on_glide.speed_ms - 3.0).abs() < 0.03, "{on_glide:?}");
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
    fn velocities_that_ring_a_point_in_no_order_are_not_a_circle() {
        // Pulling up and pushing over while weaving, or flying straight
        // through rough air: the velocities lie around one point, at about
        // one distance from it, and fit a circle well. But they visit it in
        // no order, and a glider on a circle goes round one way. Recorded
        // flights gave a wind of 127 km/h this way before the estimator
        // asked which way round.
        let ring = |centre: (f64, f64)| {
            let mut estimator = WindEstimator::new();
            let mut seed = 5;
            let mut measured = 0;
            for second in 0..240 {
                let heading = TAU * noise(&mut seed);
                let radius = 6.0 + 0.4 * noise(&mut seed);
                if estimator
                    .add(
                        f64::from(second),
                        centre.0 + radius * heading.sin(),
                        centre.1 + radius * heading.cos(),
                    )
                    .is_some()
                {
                    measured += 1;
                }
            }
            (measured, estimator.wind())
        };
        // Around a wind a glider could circle in.
        assert_eq!(ring((8.0, 3.0)), (0, None));
        // And around a speed over the ground no wind has.
        assert_eq!(ring((34.0, -9.0)), (0, None));
    }

    #[test]
    fn a_circle_recorded_every_four_seconds_gives_the_wind_too() {
        // Many recorders write a position every four or five seconds: five
        // or six to a circle, and never one in each sector. What lies
        // between two of them was flown through as well.
        let mut estimator = WindEstimator::new();
        let mut first = None;
        for step in 0..15 {
            let time = f64::from(step) * 4.0;
            let heading = TAU * time / 22.0;
            let measured =
                estimator.add(time, 10.0 * heading.sin() + 3.0, 10.0 * heading.cos() - 2.0);
            if measured.is_some() && first.is_none() {
                first = Some(time);
            }
        }
        let fix = estimator.latest().unwrap();
        // Blowing towards the east-south-east: from 303.7 degrees.
        assert!((fix.wind.from_deg - 303.7).abs() < 1.0, "{fix:?}");
        assert!(
            (fix.wind.speed_ms - 13.0_f64.sqrt()).abs() < 0.05,
            "{fix:?}"
        );
        assert!((fix.airspeed_ms - 10.0).abs() < 0.05, "{fix:?}");
        // Known within a circle and a half.
        assert!(first.is_some_and(|time| time <= 32.0), "{first:?}");
    }

    #[test]
    fn a_wind_far_stronger_than_the_glider_is_fast_is_refused() {
        // Three times the airspeed: the fit is as clean as any, and no
        // glider circles in it.
        let mut estimator = WindEstimator::new();
        circle(&mut estimator, 60, 8.0, (0.0, -24.0));
        assert_eq!(estimator.wind(), None);
        // Twice is still taken.
        let mut estimator = WindEstimator::new();
        circle(&mut estimator, 60, 8.0, (0.0, -15.0));
        assert!(estimator.wind().is_some());
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

    /// A glide heading `from_deg`, a turn to `to_deg` at `rate` degrees a
    /// second, and the glide on, at one speed through the air in a wind
    /// blowing toward `wind`, with `rough` metres per second of noise.
    fn glide_with_a_turn(
        from_deg: f64,
        to_deg: f64,
        rate: f64,
        airspeed: f64,
        wind: (f64, f64),
        rough: f64,
        seed: u64,
    ) -> Option<WindFix> {
        let mut estimator = GlideWindEstimator::new();
        let mut seed = seed;
        let turn_s = ((to_deg - from_deg).abs() / rate).round() as i32;
        let mut last = None;
        for second in 0..(40 + turn_s) {
            let heading: f64 = if second < 20 {
                from_deg
            } else if second < 20 + turn_s {
                from_deg + (to_deg - from_deg).signum() * rate * f64::from(second - 20)
            } else {
                to_deg
            };
            let (sin, cos) = heading.to_radians().sin_cos();
            let fix = estimator.add(
                f64::from(second),
                airspeed * sin + wind.0 + rough * noise(&mut seed),
                airspeed * cos + wind.1 + rough * noise(&mut seed),
            );
            last = fix.or(last);
        }
        last
    }

    #[test]
    fn a_turn_of_a_third_of_a_circle_on_a_glide_gives_the_wind() {
        // North, then a turn to the east-south-east, at 10.5 m/s in a
        // westerly of 4.
        let fix = glide_with_a_turn(0.0, 130.0, 5.0, 10.5, (4.0, 0.0), 0.0, 1).unwrap();
        assert!((fix.wind.from_deg - 270.0).abs() < 1.0, "{fix:?}");
        assert!((fix.wind.speed_ms - 4.0).abs() < 0.05, "{fix:?}");
        assert!((fix.airspeed_ms - 10.5).abs() < 0.05, "{fix:?}");
        // Trusted less than a circle: at most the share of the circle seen.
        assert!(fix.quality < 0.5, "{fix:?}");

        // With the noise of a receiver, over many made-up flights: within
        // 1.5 m/s of the wind, or silent.
        let mut said = 0;
        for seed in 1..=40 {
            if let Some(fix) = glide_with_a_turn(20.0, 170.0, 4.0, 10.5, (4.0, 0.0), 0.3, seed) {
                said += 1;
                let (sin, cos) = fix.wind.from_deg.to_radians().sin_cos();
                let (east, north) = (-fix.wind.speed_ms * sin, -fix.wind.speed_ms * cos);
                let off = (east - 4.0).hypot(north);
                assert!(off < 1.5, "seed {seed}: off by {off}: {fix:?}");
            }
        }
        assert!(said >= 30, "{said}");
    }

    #[test]
    fn less_than_a_third_of_a_circle_says_nothing() {
        for seed in 1..=20 {
            let fix = glide_with_a_turn(0.0, 80.0, 4.0, 10.5, (4.0, 0.0), 0.3, seed);
            assert_eq!(fix, None, "seed {seed}");
        }
    }

    #[test]
    fn a_straight_line_gives_no_wind_whatever_the_speed_does_on_it() {
        // Straight, with the noise of a receiver.
        let mut estimator = GlideWindEstimator::new();
        let mut seed = 11;
        for second in 0..600 {
            let fix = estimator.add(
                f64::from(second),
                8.0 + noise(&mut seed),
                6.0 + noise(&mut seed),
            );
            assert_eq!(fix, None, "second {second}");
        }
        // Straight, on and off the speed bar: the ground speed goes from
        // 10 to 15 m/s and back, and that is a line, not an arc.
        let mut estimator = GlideWindEstimator::new();
        for second in 0..600 {
            let speed = 12.5 + 2.5 * (f64::from(second) / 20.0).sin();
            let fix = estimator.add(
                f64::from(second),
                speed * 0.6 + 0.5 * noise(&mut seed),
                speed * 0.8 + 0.5 * noise(&mut seed),
            );
            assert_eq!(fix, None, "second {second}");
        }
        // Velocities that ring a point in no order are no arc either.
        let mut estimator = GlideWindEstimator::new();
        for second in 0..240 {
            let heading = TAU * noise(&mut seed);
            let fix = estimator.add(
                f64::from(second),
                8.0 + 7.0 * heading.sin(),
                3.0 + 7.0 * heading.cos(),
            );
            assert_eq!(fix, None, "second {second}");
        }
    }

    #[test]
    fn a_wide_s_on_a_glide_gives_the_wind_too() {
        // Right through 140 degrees and back again: the arc is flown
        // twice, once each way round.
        let mut estimator = GlideWindEstimator::new();
        let mut last = None;
        for second in 0..60 {
            let heading: f64 = 140.0 * (TAU * f64::from(second) / 120.0).sin().abs();
            let (sin, cos) = heading.to_radians().sin_cos();
            let fix = estimator.add(f64::from(second), 11.0 * sin - 2.0, 11.0 * cos - 3.0);
            last = fix.or(last);
        }
        let fix = last.unwrap();
        // Blowing toward the south-south-west: from 33.7 degrees.
        assert!((fix.wind.from_deg - 33.7).abs() < 1.0, "{fix:?}");
        assert!(
            (fix.wind.speed_ms - 13.0_f64.sqrt()).abs() < 0.05,
            "{fix:?}"
        );
    }
}
