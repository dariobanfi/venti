//! A glider's polar: how fast it sinks at each speed through the air, and
//! what follows from it: the air's own rise or fall (netto), and the speed
//! to fly.
//!
//! The polar is a parabola, sink = a·v² + b·v + c, which is what gliding
//! computers have always taken and what three measured points allow. A
//! pilot gives it as three points, or as two: trim speed with its sink and
//! top speed with its sink. For two points the third condition is that
//! the glider's best glide in still air is at trim speed, which holds for
//! most paragliders to within a few km/h; *to verify with pilots*.
//!
//! Makers publish no polars, so the numbers per class in
//! [`Polar::for_class`] are the project's own reading of what wings of a
//! class do, each *to verify with pilots*. They are a place to start
//! from; a pilot's own numbers replace them.
//!
//! What the polar does not know: the pilot's weight on the wing, the
//! harness, a wet or old wing, and the bank in a turn. In a turn a glider
//! sinks faster than its polar says, so netto and the speed to fly are
//! for the glide only.

use super::Wind;
use super::navigation::{Glides, OnCourse, speed_towards};
use super::wind::solve;

/// The class a paraglider is certified in, for a polar to start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GliderClass {
    /// EN A.
    A,
    /// EN B.
    B,
    /// EN C.
    C,
    /// EN D.
    D,
    /// CCC, the competition class.
    Ccc,
}

/// A polar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Polar {
    a: f64,
    b: f64,
    c: f64,
    /// The speed with the brakes up, metres per second.
    pub trim_speed_ms: f64,
    /// The slowest speed the polar is read at, metres per second: the
    /// speed of least sink, or the slowest point given.
    pub min_speed_ms: f64,
    /// The fastest speed, metres per second: full speed bar.
    pub max_speed_ms: f64,
}

/// The speeds a polar may be given for, metres per second: 18 to 90 km/h.
const SPEEDS_MS: std::ops::RangeInclusive<f64> = 5.0..=25.0;

/// The sinks a polar may be given, metres per second.
const SINKS_MS: std::ops::RangeInclusive<f64> = 0.5..=6.0;

/// The speeds between the slowest and the fastest are tried this far
/// apart, metres per second, when the best one is looked for.
const SPEED_STEP_MS: f64 = 0.1;

/// The air counts as sinking from this fall on, metres per second.
pub const SINKING_AIR_MS: f64 = -0.5;

/// And as rising from this rise on.
pub const RISING_AIR_MS: f64 = 0.3;

/// A wind along the course counts from this speed on, metres per second:
/// 7 km/h.
pub const WIND_COUNTS_MS: f64 = 2.0;

impl Polar {
    /// A polar through three points, each a speed through the air and
    /// the sink at it, both in metres per second and both positive, in
    /// any order. The trim speed is taken as the speed of best glide.
    /// `None` when the points make no polar: two at one speed, speeds or
    /// sinks no paraglider has, or a curve that does not bend upward.
    pub fn from_points(points: [(f64, f64); 3]) -> Option<Self> {
        let mut points = points;
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        let sane = points
            .iter()
            .all(|(speed, sink)| SPEEDS_MS.contains(speed) && SINKS_MS.contains(sink));
        if !sane || points[1].0 - points[0].0 < 0.5 || points[2].0 - points[1].0 < 0.5 {
            return None;
        }
        let [a, b, c] = solve(
            [
                [points[0].0.powi(2), points[0].0, 1.0],
                [points[1].0.powi(2), points[1].0, 1.0],
                [points[2].0.powi(2), points[2].0, 1.0],
            ],
            [points[0].1, points[1].1, points[2].1],
        )?;
        let (slowest, fastest) = (points[0].0, points[2].0);
        // The speed of least sink, where it lies among the points.
        let least_sink = if a > 0.0 { -b / (2.0 * a) } else { slowest };
        let mut polar = Self {
            a,
            b,
            c,
            trim_speed_ms: slowest,
            min_speed_ms: least_sink.clamp(slowest, fastest),
            max_speed_ms: fastest,
        };
        polar.trim_speed_ms = polar.best(Some, 0.0)?.0;
        polar.checked()
    }

    /// A polar from two points: trim speed with its sink and top speed
    /// with its sink, metres per second, taking the best glide in still
    /// air to be at trim speed. `None` when the numbers make no polar:
    /// above all when the glide at top speed is not worse than at trim.
    pub fn from_trim_and_max(
        trim_speed_ms: f64,
        trim_sink_ms: f64,
        max_speed_ms: f64,
        max_sink_ms: f64,
    ) -> Option<Self> {
        let sane = SPEEDS_MS.contains(&trim_speed_ms)
            && SPEEDS_MS.contains(&max_speed_ms)
            && SINKS_MS.contains(&trim_sink_ms)
            && SINKS_MS.contains(&max_sink_ms);
        if !sane || max_speed_ms - trim_speed_ms < 1.0 {
            return None;
        }
        // Best glide at trim means c = a·trim²; the two points give a and b.
        let a = (max_sink_ms - trim_sink_ms * max_speed_ms / trim_speed_ms)
            / (max_speed_ms - trim_speed_ms).powi(2);
        if a <= 0.0 {
            return None;
        }
        let b = (trim_sink_ms - 2.0 * a * trim_speed_ms.powi(2)) / trim_speed_ms;
        Self {
            a,
            b,
            c: a * trim_speed_ms.powi(2),
            trim_speed_ms,
            // The speed of least sink, and no slower than three quarters
            // of trim: below that a paraglider is near its stall, and the
            // parabola knows nothing of a stall.
            min_speed_ms: (-b / (2.0 * a)).clamp(0.75 * trim_speed_ms, trim_speed_ms),
            max_speed_ms,
        }
        .checked()
    }

    /// A polar to start from for a wing of a class, as trim speed with
    /// its glide ratio and top speed with its glide ratio. The numbers
    /// are the project's own, *to verify with pilots*:
    ///
    /// | Class | Trim | Glide | Top speed | Glide |
    /// |---|---|---|---|---|
    /// | EN A | 37 km/h | 8.0 | 46 km/h | 6.0 |
    /// | EN B | 38 km/h | 9.0 | 50 km/h | 6.5 |
    /// | EN C | 39 km/h | 10.0 | 55 km/h | 7.0 |
    /// | EN D | 40 km/h | 10.5 | 58 km/h | 7.3 |
    /// | CCC | 41 km/h | 11.5 | 65 km/h | 7.5 |
    pub fn for_class(class: GliderClass) -> Self {
        let (trim_kmh, trim_glide, max_kmh, max_glide) = match class {
            GliderClass::A => (37.0, 8.0, 46.0, 6.0),
            GliderClass::B => (38.0, 9.0, 50.0, 6.5),
            GliderClass::C => (39.0, 10.0, 55.0, 7.0),
            GliderClass::D => (40.0, 10.5, 58.0, 7.3),
            GliderClass::Ccc => (41.0, 11.5, 65.0, 7.5),
        };
        let (trim, max): (f64, f64) = (trim_kmh / 3.6, max_kmh / 3.6);
        Self::from_trim_and_max(trim, trim / trim_glide, max, max / max_glide)
            .expect("the classes' numbers make a polar")
    }

    /// The polar if it sinks at every speed it is read at.
    fn checked(self) -> Option<Self> {
        let sinks = [self.min_speed_ms, self.trim_speed_ms, self.max_speed_ms]
            .iter()
            .all(|&speed| self.sink_ms(speed) > 0.2);
        (sinks && self.a.is_finite() && self.b.is_finite() && self.c.is_finite()).then_some(self)
    }

    /// The sink at a speed through the air, metres per second, positive
    /// downward. A speed outside the polar's is read at its nearest end.
    pub fn sink_ms(&self, airspeed_ms: f64) -> f64 {
        let speed = airspeed_ms.clamp(self.min_speed_ms, self.max_speed_ms);
        self.a * speed * speed + self.b * speed + self.c
    }

    /// The glide ratio in still air at trim speed.
    pub fn trim_glide(&self) -> f64 {
        self.trim_speed_ms / self.sink_ms(self.trim_speed_ms)
    }

    /// How fast the air itself rises, metres per second, negative when it
    /// sinks: the glider's climb with the polar's sink at its airspeed
    /// given back. For straight flight; in a turn the glider sinks faster
    /// than its polar says and the air seems worse than it is.
    pub fn netto_ms(&self, climb_ms: f64, airspeed_ms: f64) -> f64 {
        climb_ms + self.sink_ms(airspeed_ms)
    }

    /// The speed between the slowest and the fastest that goes furthest
    /// over the ground per metre of height, with that glide ratio.
    /// `over_ground` gives the speed over the ground for a speed through
    /// the air, `None` where there is no way forward; `falling_ms` is
    /// added to the polar's sink.
    fn best(
        &self,
        over_ground: impl Fn(f64) -> Option<f64>,
        falling_ms: f64,
    ) -> Option<(f64, f64)> {
        let steps = ((self.max_speed_ms - self.min_speed_ms) / SPEED_STEP_MS).ceil() as usize;
        (0..=steps)
            .filter_map(|step| {
                let speed =
                    (self.min_speed_ms + step as f64 * SPEED_STEP_MS).min(self.max_speed_ms);
                let falls = self.sink_ms(speed) + falling_ms;
                let ground = over_ground(speed)?;
                (falls > 0.0 && ground > 0.0).then(|| (speed, ground / falls))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The speed to fly on a glide, by MacCready's rule: the speed that
    /// loses the least height per metre over the ground, in air that
    /// rises at `air_ms` (negative: sinks), against a head wind of
    /// `head_wind_ms` (negative: a tail wind), expecting a climb of
    /// `expected_climb_ms` at the end of the glide (zero: fly for
    /// distance).
    pub fn speed_to_fly(
        &self,
        head_wind_ms: f64,
        air_ms: f64,
        expected_climb_ms: f64,
    ) -> SpeedToFly {
        let sinking_air = air_ms <= SINKING_AIR_MS;
        let head_wind = head_wind_ms >= WIND_COUNTS_MS;
        let tail_wind = head_wind_ms <= -WIND_COUNTS_MS;
        // Air that rises faster than the glider sinks at trim: every speed
        // climbs, and there is nothing to hurry away from.
        let rising_air = air_ms >= RISING_AIR_MS;
        let speed_ms = if air_ms >= self.sink_ms(self.trim_speed_ms) {
            self.trim_speed_ms
        } else {
            self.best(
                |speed| Some(speed - head_wind_ms).filter(|ground| *ground > 0.5),
                expected_climb_ms.max(0.0) - air_ms,
            )
            // No way forward at any speed: as fast as the wing goes.
            .map_or(self.max_speed_ms, |(speed, _)| speed)
        };
        SpeedToFly {
            speed_ms,
            bar: ((speed_ms - self.trim_speed_ms) / (self.max_speed_ms - self.trim_speed_ms))
                .clamp(0.0, 1.0),
            sinking_air,
            rising_air,
            head_wind,
            tail_wind,
        }
    }
}

impl Glides for Polar {
    /// On a course in a wind the polar is flown at the speed that carries
    /// furthest over the ground: faster into a head wind, at trim or
    /// slower with the wind from behind.
    fn on_course(&self, course_deg: f64, wind: Option<Wind>) -> Option<OnCourse> {
        let (airspeed_ms, _) = self.best(|speed| speed_towards(course_deg, wind, speed), 0.0)?;
        Some(OnCourse {
            ground_speed_ms: speed_towards(course_deg, wind, airspeed_ms)?,
            airspeed_ms,
            sink_ms: self.sink_ms(airspeed_ms),
        })
    }
}

/// The speed to fly, and why.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeedToFly {
    /// The speed through the air, metres per second.
    pub speed_ms: f64,
    /// Where that lies between trim speed, 0, and top speed, 1: how much
    /// speed bar.
    pub bar: f64,
    /// Whether the air sinks, by [`SINKING_AIR_MS`].
    pub sinking_air: bool,
    /// Whether the air rises, by [`RISING_AIR_MS`].
    pub rising_air: bool,
    /// Whether there is a head wind, by [`WIND_COUNTS_MS`].
    pub head_wind: bool,
    /// Whether there is a tail wind.
    pub tail_wind: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_points_make_a_polar_with_its_best_glide_at_trim() {
        let polar = Polar::for_class(GliderClass::B);
        let trim = 38.0 / 3.6;
        assert!((polar.sink_ms(trim) - trim / 9.0).abs() < 1e-9);
        assert!((polar.sink_ms(50.0 / 3.6) - 50.0 / 3.6 / 6.5).abs() < 1e-9);
        assert!((polar.trim_glide() - 9.0).abs() < 1e-9);
        // No speed glides better in still air.
        for tenth in 80..=139 {
            let speed = f64::from(tenth) / 10.0;
            assert!(speed / polar.sink_ms(speed) <= 9.0 + 1e-9, "{speed}");
        }
        // It sinks least a little slower than trim, and not by much less.
        assert!(polar.min_speed_ms < trim && polar.min_speed_ms > 0.75 * trim);
        let least = polar.sink_ms(polar.min_speed_ms);
        assert!(least < trim / 9.0 && least > 1.0, "{least}");
    }

    #[test]
    fn every_class_has_a_polar_and_a_higher_class_glides_better() {
        let classes = [
            GliderClass::A,
            GliderClass::B,
            GliderClass::C,
            GliderClass::D,
            GliderClass::Ccc,
        ];
        let glides: Vec<f64> = classes
            .iter()
            .map(|class| Polar::for_class(*class).trim_glide())
            .collect();
        assert!(
            glides.windows(2).all(|pair| pair[0] < pair[1]),
            "{glides:?}"
        );
    }

    #[test]
    fn three_points_make_a_polar_and_nonsense_makes_none() {
        // Least sink 1.05 at 32 km/h, 1.15 at 38, 2.2 at 52.
        let polar = Polar::from_points([(38.0 / 3.6, 1.15), (32.0 / 3.6, 1.05), (52.0 / 3.6, 2.2)])
            .unwrap();
        assert!((polar.sink_ms(38.0 / 3.6) - 1.15).abs() < 1e-9);
        assert!((polar.sink_ms(52.0 / 3.6) - 2.2).abs() < 1e-9);
        assert!((polar.max_speed_ms - 52.0 / 3.6).abs() < 1e-9);
        // The best glide lies between the points, near 37 km/h.
        assert!((9.0..11.5).contains(&polar.trim_speed_ms), "{polar:?}");

        // A wing that glides better the faster it goes, two points at one
        // speed, a sink of nothing.
        assert_eq!(Polar::from_trim_and_max(10.0, 1.2, 14.0, 1.5), None);
        assert_eq!(
            Polar::from_points([(10.0, 1.1), (10.1, 1.2), (14.0, 2.0)]),
            None
        );
        assert_eq!(Polar::from_trim_and_max(10.0, 0.0, 14.0, 2.0), None);
    }

    #[test]
    fn netto_gives_the_polars_sink_back() {
        let polar = Polar::for_class(GliderClass::B);
        let trim = polar.trim_speed_ms;
        // Sinking as the polar says: the air stands still.
        assert!(polar.netto_ms(-polar.sink_ms(trim), trim).abs() < 1e-9);
        // Sinking 2.5 at trim: the air falls by 1.33.
        assert!((polar.netto_ms(-2.5, trim) + 2.5 - trim / 9.0).abs() < 1e-9);
        // Level at trim: the air rises as fast as the glider sinks.
        assert!((polar.netto_ms(0.0, trim) - trim / 9.0).abs() < 1e-9);
    }

    #[test]
    fn the_speed_to_fly_rises_with_sinking_air_and_a_head_wind() {
        let polar = Polar::for_class(GliderClass::C);
        let trim = polar.trim_speed_ms;
        // Still air, no wind: trim.
        let calm = polar.speed_to_fly(0.0, 0.0, 0.0);
        assert!((calm.speed_ms - trim).abs() <= SPEED_STEP_MS, "{calm:?}");
        assert!(calm.bar < 0.05 && !calm.sinking_air && !calm.head_wind);
        // Sinking air: faster. A head wind: faster. Both: faster still.
        let sink = polar.speed_to_fly(0.0, -1.5, 0.0);
        let head = polar.speed_to_fly(6.0, 0.0, 0.0);
        let both = polar.speed_to_fly(6.0, -1.5, 0.0);
        assert!(sink.speed_ms > trim + 1.0 && sink.sinking_air, "{sink:?}");
        assert!(head.speed_ms > trim + 0.5 && head.head_wind, "{head:?}");
        assert!(both.speed_ms > sink.speed_ms.max(head.speed_ms), "{both:?}");
        assert!(both.bar > sink.bar && both.bar <= 1.0);
        // A tail wind: no faster than trim. Rising air: trim.
        let tail = polar.speed_to_fly(-6.0, 0.0, 0.0);
        assert!(
            tail.speed_ms <= trim + SPEED_STEP_MS && tail.tail_wind,
            "{tail:?}"
        );
        let lift = polar.speed_to_fly(0.0, 2.0, 0.0);
        assert!(
            (lift.speed_ms - trim).abs() < 1e-9 && lift.rising_air,
            "{lift:?}"
        );
        // A strong climb expected at the end: faster, as MacCready has it.
        let racing = polar.speed_to_fly(0.0, 0.0, 3.0);
        assert!(racing.speed_ms > trim + 1.0, "{racing:?}");
        // A wind no speed makes way against: everything the wing has.
        let stuck = polar.speed_to_fly(20.0, 0.0, 0.0);
        assert!(
            (stuck.speed_ms - polar.max_speed_ms).abs() < 1e-9,
            "{stuck:?}"
        );
    }

    #[test]
    fn into_a_head_wind_a_polar_glides_further_than_trim_speed_does() {
        use crate::live::navigation::Glider;
        let polar = Polar::for_class(GliderClass::C);
        let trim_only = Glider {
            trim_speed_ms: polar.trim_speed_ms,
            glide_ratio: polar.trim_glide(),
        };
        let northerly = Some(Wind {
            from_deg: 0.0,
            speed_ms: 8.0,
        });
        // Still air: the same glide.
        let still = polar.on_course(0.0, None).unwrap();
        assert!(
            (still.glide() - polar.trim_glide()).abs() < 0.01,
            "{still:?}"
        );
        // Into 8 m/s: faster through the air, and further over the ground
        // than at trim.
        let polar_made = polar.on_course(0.0, northerly).unwrap();
        let trim_made = trim_only.on_course(0.0, northerly).unwrap();
        assert!(
            polar_made.airspeed_ms > polar.trim_speed_ms + 1.0,
            "{polar_made:?}"
        );
        assert!(
            polar_made.glide() > trim_made.glide() * 1.1,
            "{polar_made:?} {trim_made:?}"
        );
        // A wind stronger than the wing's top speed: no way.
        let gale = Some(Wind {
            from_deg: 0.0,
            speed_ms: 19.0,
        });
        assert_eq!(polar.on_course(0.0, gale), None);
    }
}
