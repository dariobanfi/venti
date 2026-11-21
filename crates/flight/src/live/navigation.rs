//! Navigating to a place: how far, which way, and whether the glider gets
//! there.
//!
//! This is a pure calculation from the state of the flight, a target and
//! what the glider can do. It is advisory: the prediction assumes that the
//! air neither rises nor sinks on the way and that the wind stays as last
//! measured, which in the mountains it does not.

use venti_domain::GeoPoint;

use super::Wind;
use super::polar::Polar;

/// A place to fly to.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// Its name.
    pub name: String,
    /// Where it is.
    pub position: GeoPoint,
    /// Its height above sea level, metres, if known. Without it nothing
    /// can be said about arriving above or below it.
    pub elevation_m: Option<f64>,
}

/// What the glider does on a straight glide in still air.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glider {
    /// Speed through the air with the brakes up, metres per second.
    pub trim_speed_ms: f64,
    /// Metres forward per metre of height lost at that speed.
    pub glide_ratio: f64,
}

impl Default for Glider {
    /// 38 km/h and a glide of 8. Manufacturers state 9 to 10 for a wing of
    /// the middle classes at about that speed; 8 leaves room for the pilot
    /// not flying a perfectly straight line. To verify with pilots; the
    /// app lets each pilot set their own.
    fn default() -> Self {
        Self {
            trim_speed_ms: 38.0 / 3.6,
            glide_ratio: 8.0,
        }
    }
}

/// Where the target is and what it takes to get there.
#[derive(Debug, Clone, PartialEq)]
pub struct Navigation {
    /// The target's name.
    pub name: String,
    /// Distance to it over the ground, metres.
    pub distance_m: f64,
    /// The direction to fly, degrees clockwise from north.
    pub bearing_deg: f64,
    /// How far the glider is above the target now, metres. Negative when
    /// below it. `None` without the target's height or the glider's.
    pub height_above_m: Option<f64>,
    /// The glide ratio over the ground needed to arrive at the target's
    /// height. `None` when the glider is not above the target.
    pub required_glide: Option<f64>,
    /// The glide ratio over the ground the glider is expected to make
    /// towards the target, at trim speed in the wind as last measured.
    /// `None` when the wind is too strong to make any way towards it.
    pub expected_glide: Option<f64>,
    /// How far above the target the glider is expected to arrive, metres.
    /// Negative when it is expected not to reach it.
    pub arrival_height_m: Option<f64>,
    /// How long the glide takes, seconds.
    pub time_to_go_s: Option<f64>,
    /// Whether a wind was known for the prediction. Without one, still air
    /// is assumed.
    pub wind_known: bool,
}

/// What a glider makes on a course it holds in a wind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnCourse {
    /// Its speed along the course over the ground, metres per second.
    pub ground_speed_ms: f64,
    /// Its speed through the air, metres per second.
    pub airspeed_ms: f64,
    /// The height it loses each second, metres.
    pub sink_ms: f64,
}

impl OnCourse {
    /// Metres forward over the ground for each metre of height lost.
    pub fn glide(&self) -> f64 {
        self.ground_speed_ms / self.sink_ms
    }
}

/// Something that glides: a glider known by its trim speed and glide
/// ratio, or by its polar.
pub trait Glides {
    /// What it makes on a course held in a wind. `None` when the wind
    /// lets it make no way on that course.
    fn on_course(&self, course_deg: f64, wind: Option<Wind>) -> Option<OnCourse>;
}

impl Glides for Glider {
    fn on_course(&self, course_deg: f64, wind: Option<Wind>) -> Option<OnCourse> {
        speed_towards(course_deg, wind, self.trim_speed_ms).map(|speed| OnCourse {
            ground_speed_ms: speed,
            airspeed_ms: self.trim_speed_ms,
            sink_ms: self.trim_speed_ms / self.glide_ratio,
        })
    }
}

/// How the glider's glide is known to the cockpit: by the two numbers of
/// a trim speed and a glide ratio, or by a polar the pilot has set. With
/// a polar a glide is flown at the speed that carries furthest over the
/// ground in the wind on its course.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Performance {
    /// Trim speed and glide ratio.
    Trim(Glider),
    /// A polar.
    Polar(Polar),
}

impl Glides for Performance {
    fn on_course(&self, course_deg: f64, wind: Option<Wind>) -> Option<OnCourse> {
        match self {
            Self::Trim(glider) => glider.on_course(course_deg, wind),
            Self::Polar(polar) => polar.on_course(course_deg, wind),
        }
    }
}

/// Works out the way to a target from where the glider is.
pub fn navigate(
    position: GeoPoint,
    altitude_m: Option<f64>,
    wind: Option<Wind>,
    target: &Target,
    glider: &impl Glides,
) -> Navigation {
    let distance_m = position.distance_m(target.position);
    let bearing_deg = bearing_deg(position, target.position);
    let height_above_m = altitude_m
        .zip(target.elevation_m)
        .map(|(altitude, elevation)| altitude - elevation);

    let made = glider.on_course(bearing_deg, wind);
    let time_to_go_s = made.map(|made| distance_m / made.ground_speed_ms);

    Navigation {
        name: target.name.clone(),
        distance_m,
        bearing_deg,
        height_above_m,
        required_glide: height_above_m
            .filter(|height| *height > 0.0)
            .map(|height| distance_m / height),
        expected_glide: made.map(|made| made.glide()),
        arrival_height_m: height_above_m
            .zip(made)
            .map(|(height, made)| height - distance_m / made.glide()),
        time_to_go_s,
        wind_known: wind.is_some(),
    }
}

/// Metres forward over the ground for each metre of height lost, on a
/// course held in a wind: at the glider's trim speed, or with a polar at
/// the speed that carries furthest. `None` when the wind lets the glider
/// make no way on that course.
pub fn glide_over_ground(course_deg: f64, wind: Option<Wind>, glider: &impl Glides) -> Option<f64> {
    glider.on_course(course_deg, wind).map(|made| made.glide())
}

/// The speed over the ground along a course, flying at an airspeed and
/// holding the course against the wind's push to the side. `None` when the
/// wind is too strong: the glider cannot hold the course, or is carried
/// backwards on it.
pub(crate) fn speed_towards(course_deg: f64, wind: Option<Wind>, airspeed_ms: f64) -> Option<f64> {
    let Some(wind) = wind else {
        return Some(airspeed_ms);
    };
    // The wind blows towards the opposite of where it comes from. Split it
    // into the part along the course and the part across it.
    let off_course = (wind.from_deg + 180.0 - course_deg).to_radians();
    let along = wind.speed_ms * off_course.cos();
    let across = wind.speed_ms * off_course.sin();
    // Part of the airspeed is spent holding the course against the
    // crosswind; what is left moves the glider forward.
    let forward = (airspeed_ms * airspeed_ms - across * across).sqrt();
    let speed = forward + along;
    // Under a walking pace the glide never ends; the square root of a
    // negative number (crosswind above airspeed) is NaN and fails this too.
    (speed > 1.0).then_some(speed)
}

/// The direction from one point to another, degrees clockwise from north.
pub fn bearing_deg(from: GeoPoint, to: GeoPoint) -> f64 {
    let (lat1, lat2) = (from.lat().to_radians(), to.lat().to_radians());
    let along = (to.lon() - from.lon()).to_radians();
    let east = along.sin() * lat2.cos();
    let north = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * along.cos();
    east.atan2(north).to_degrees().rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    /// A landing field 8 km due east of the glider, at 800 m.
    fn landing() -> Target {
        Target {
            name: "Landing".to_owned(),
            // One degree of longitude at 47 degrees north is 75.8 km.
            position: point(47.0, 11.0 + 8.0 / 75.83),
            elevation_m: Some(800.0),
        }
    }

    const GLIDER: Glider = Glider {
        trim_speed_ms: 10.0,
        glide_ratio: 8.0,
    };

    #[test]
    fn distance_bearing_and_the_glide_needed_follow_from_the_geometry() {
        let way = navigate(point(47.0, 11.0), Some(1800.0), None, &landing(), &GLIDER);

        assert_eq!(way.name, "Landing");
        assert!((way.distance_m - 8000.0).abs() < 20.0, "{}", way.distance_m);
        assert!((way.bearing_deg - 90.0).abs() < 0.1, "{}", way.bearing_deg);
        assert_eq!(way.height_above_m, Some(1000.0));
        // 8 km with 1000 m to lose.
        assert!((way.required_glide.unwrap() - 8.0).abs() < 0.03);
        // In still air a glide of 8 uses exactly that: arrival at the
        // field's height, after 800 seconds at 10 m/s.
        assert!(!way.wind_known);
        assert_eq!(way.expected_glide, Some(8.0));
        assert!(way.arrival_height_m.unwrap().abs() < 3.0);
        assert!((way.time_to_go_s.unwrap() - 800.0).abs() < 2.0);
    }

    #[test]
    fn a_headwind_costs_height_and_a_tailwind_gives_it() {
        let from = point(47.0, 11.0);
        // 5 m/s from the east, on the nose: half the speed over the ground.
        let headwind = Wind {
            from_deg: 90.0,
            speed_ms: 5.0,
        };
        let against = navigate(from, Some(1800.0), Some(headwind), &landing(), &GLIDER);
        assert!(against.wind_known);
        assert!((against.expected_glide.unwrap() - 4.0).abs() < 0.01);
        // Twice the time, twice the height lost: 1000 m short.
        assert!((against.arrival_height_m.unwrap() + 1000.0).abs() < 6.0);

        // The same wind from behind: half as much again over the ground.
        let tailwind = Wind {
            from_deg: 270.0,
            speed_ms: 5.0,
        };
        let with = navigate(from, Some(1800.0), Some(tailwind), &landing(), &GLIDER);
        assert!((with.expected_glide.unwrap() - 12.0).abs() < 0.01);
        assert!((with.arrival_height_m.unwrap() - 333.0).abs() < 3.0);
    }

    #[test]
    fn a_crosswind_costs_a_little_and_too_much_wind_makes_the_target_unreachable() {
        let from = point(47.0, 11.0);
        // 6 m/s from the north, square to the course: the glider crabs,
        // and 8 of its 10 m/s are left along the course.
        let crosswind = Wind {
            from_deg: 0.0,
            speed_ms: 6.0,
        };
        let across = navigate(from, Some(1800.0), Some(crosswind), &landing(), &GLIDER);
        assert!((across.expected_glide.unwrap() - 6.4).abs() < 0.01);

        // A crosswind stronger than the glider flies, and a headwind that
        // carries it backwards: no way of getting there, and no numbers
        // that pretend otherwise.
        for wind in [
            Wind {
                from_deg: 0.0,
                speed_ms: 11.0,
            },
            Wind {
                from_deg: 90.0,
                speed_ms: 10.5,
            },
            Wind {
                from_deg: 90.0,
                speed_ms: 9.5,
            },
        ] {
            let way = navigate(from, Some(1800.0), Some(wind), &landing(), &GLIDER);
            assert_eq!(way.expected_glide, None, "{wind:?}");
            assert_eq!(way.arrival_height_m, None, "{wind:?}");
            assert_eq!(way.time_to_go_s, None, "{wind:?}");
            // Where the target is does not depend on the wind.
            assert!((way.distance_m - 8000.0).abs() < 20.0);
        }
    }

    #[test]
    fn without_heights_only_the_way_is_given() {
        let mut unknown = landing();
        unknown.elevation_m = None;
        let way = navigate(point(47.0, 11.0), Some(1800.0), None, &unknown, &GLIDER);
        assert_eq!(way.height_above_m, None);
        assert_eq!(way.required_glide, None);
        assert_eq!(way.arrival_height_m, None);
        assert!(way.time_to_go_s.is_some());

        // No altitude yet, as before the first good fix.
        let way = navigate(point(47.0, 11.0), None, None, &landing(), &GLIDER);
        assert_eq!(way.height_above_m, None);

        // Below the target there is no glide ratio that gets there.
        let way = navigate(point(47.0, 11.0), Some(700.0), None, &landing(), &GLIDER);
        assert_eq!(way.height_above_m, Some(-100.0));
        assert_eq!(way.required_glide, None);
        assert!(way.arrival_height_m.unwrap() < -1000.0);
    }

    #[test]
    fn bearings_run_clockwise_from_north() {
        let here = point(47.0, 11.0);
        assert!((bearing_deg(here, point(48.0, 11.0)) - 0.0).abs() < 1e-9);
        assert!((bearing_deg(here, point(46.0, 11.0)) - 180.0).abs() < 1e-9);
        assert!((bearing_deg(here, point(47.0, 10.0)) - 270.0).abs() < 0.5);
        // At the target itself any bearing will do, but it must be a number.
        assert!(bearing_deg(here, here).is_finite());
    }
}
