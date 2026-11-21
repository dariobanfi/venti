//! The glider through the air, worked out from its way over the ground
//! and the wind: its airspeed, the direction it points in, and whether
//! the wind carries it backwards.
//!
//! No probe and no compass: the velocity over the ground less the wind is
//! the velocity through the air. It is as good as the wind is, which was
//! measured a while ago and somewhere else, so every number here is an
//! estimate and the cockpit says so.

use super::Wind;

/// Below this speed through the air, metres per second, nothing is said:
/// the wind taken off was not the wind the glider is in.
const AIRSPEED_AT_LEAST_MS: f64 = 2.0;

/// Above this neither: 90 km/h is no paraglider's speed.
const AIRSPEED_AT_MOST_MS: f64 = 25.0;

/// The glider through the air.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThroughTheAir {
    /// Its speed through the air, metres per second.
    pub airspeed_ms: f64,
    /// The direction it points in, degrees clockwise from north.
    pub heading_deg: f64,
    /// How fast it moves over the ground against the direction it points
    /// in, metres per second: the wind carries it backwards. `None` while
    /// it makes way forward, which is the same as its course over the
    /// ground lying within 90° of its heading.
    pub backwards_ms: Option<f64>,
    /// The part of the wind against the direction it points in, metres
    /// per second: positive is a head wind, negative a tail wind.
    pub head_wind_ms: f64,
}

/// The glider through the air, from its speed and course over the ground
/// and the wind. `None` when the result is no speed a glider flies, which
/// means the wind is not the one it is in.
pub fn through_the_air(ground_speed_ms: f64, course_deg: f64, wind: Wind) -> Option<ThroughTheAir> {
    let (sin, cos) = course_deg.to_radians().sin_cos();
    let (ground_east, ground_north) = (ground_speed_ms * sin, ground_speed_ms * cos);
    // The wind blows toward the opposite of where it comes from.
    let (sin, cos) = wind.from_deg.to_radians().sin_cos();
    let (wind_east, wind_north) = (-wind.speed_ms * sin, -wind.speed_ms * cos);
    let (air_east, air_north) = (ground_east - wind_east, ground_north - wind_north);
    let airspeed_ms = air_east.hypot(air_north);
    if !(AIRSPEED_AT_LEAST_MS..=AIRSPEED_AT_MOST_MS).contains(&airspeed_ms) {
        return None;
    }
    let (point_east, point_north) = (air_east / airspeed_ms, air_north / airspeed_ms);
    let forward = ground_east * point_east + ground_north * point_north;
    Some(ThroughTheAir {
        airspeed_ms,
        heading_deg: air_east.atan2(air_north).to_degrees().rem_euclid(360.0),
        backwards_ms: (forward < 0.0).then_some(-forward),
        head_wind_ms: -(wind_east * point_east + wind_north * point_north),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_head_wind_is_added_to_the_ground_speed_and_a_cross_wind_turns_the_heading() {
        // 6 m/s north over the ground into a northerly of 4: 10 through
        // the air, pointing north.
        let air = through_the_air(
            6.0,
            0.0,
            Wind {
                from_deg: 0.0,
                speed_ms: 4.0,
            },
        )
        .unwrap();
        assert!((air.airspeed_ms - 10.0).abs() < 1e-9, "{air:?}");
        assert!(
            air.heading_deg < 1e-6 || air.heading_deg > 359.999,
            "{air:?}"
        );
        assert_eq!(air.backwards_ms, None);
        assert!((air.head_wind_ms - 4.0).abs() < 1e-9);

        // 8 m/s north over the ground in a westerly of 6: the glider
        // points 36.9 degrees left of its course, into the wind, at 10.
        let air = through_the_air(
            8.0,
            0.0,
            Wind {
                from_deg: 270.0,
                speed_ms: 6.0,
            },
        )
        .unwrap();
        assert!((air.airspeed_ms - 10.0).abs() < 1e-9, "{air:?}");
        assert!((air.heading_deg - 323.13).abs() < 0.01, "{air:?}");
        assert_eq!(air.backwards_ms, None);
    }

    #[test]
    fn a_wind_stronger_than_the_glider_carries_it_backwards() {
        // Pointing north at 9 m/s into a northerly of 10: over the ground
        // the glider moves south at 1.
        let air = through_the_air(
            1.0,
            180.0,
            Wind {
                from_deg: 0.0,
                speed_ms: 10.0,
            },
        )
        .unwrap();
        assert!((air.airspeed_ms - 9.0).abs() < 1e-9, "{air:?}");
        assert!(
            air.heading_deg < 1e-6 || air.heading_deg > 359.999,
            "{air:?}"
        );
        assert!((air.backwards_ms.unwrap() - 1.0).abs() < 1e-9, "{air:?}");

        // Crabbing: 5 m/s on a course of 20 degrees in a westerly of 6.
        // The glider points north-west, 62 degrees left of its course:
        // less than 90, so it makes way forward.
        let air = through_the_air(
            5.0,
            20.0,
            Wind {
                from_deg: 270.0,
                speed_ms: 6.0,
            },
        )
        .unwrap();
        assert!((air.heading_deg - 317.6).abs() < 0.5, "{air:?}");
        assert_eq!(air.backwards_ms, None, "{air:?}");
    }

    #[test]
    fn a_wind_that_cannot_be_the_one_the_glider_is_in_gives_nothing() {
        // Running at 15 m/s with a wind of 15 from behind: no speed
        // through the air at all.
        let none = through_the_air(
            15.0,
            90.0,
            Wind {
                from_deg: 270.0,
                speed_ms: 15.0,
            },
        );
        assert_eq!(none, None);
    }
}
