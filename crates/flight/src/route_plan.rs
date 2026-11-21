//! A route planned before the flight: when the pilot is where, what the
//! wind does along each leg, and how high to leave a point to reach the
//! next over the ground.
//!
//! Pure calculation, as the rest of the engine: no clock, no files. The
//! times are seconds after the start, the ground is the terrain a caller
//! holds, the wind what a caller read from a forecast.
//!
//! The rules, fixed and ours:
//!
//! - **Time at a point.** The distance along the legs to it over the
//!   pilot's mean speed over the ground: a cross-country speed with the
//!   climbs in it, not an airspeed. The wind is not in it: a pilot's mean
//!   speed is the one they make on days like the day planned.
//! - **The wind on a leg.** The mean, as arrows, of the wind at its two
//!   ends at the times the pilot is there ([`mean_wind`]); split into the
//!   part along the leg and the part across it ([`wind_on_course`]).
//! - **The height to leave a point at.** The lowest height from which a
//!   straight glide to the next point, at the glider's trim speed in the
//!   leg's wind, keeps `margin_m` over every place of the ground read
//!   along the line after its start, the next point's ground among them
//!   ([`departure_height`]). The air neither rises nor sinks on the way,
//!   which it does; nothing flies round a ridge.

use venti_domain::GeoPoint;

use crate::live::Wind;
use crate::live::navigation::{Glider, bearing_deg, glide_over_ground};
use crate::terrain::Terrain;

/// The ground along a leg is read this far apart, metres: some eight
/// pixels of a tile of zoom 12, close enough not to step over a ridge.
pub const GROUND_STEP_M: f64 = 100.0;

/// No leg's ground is read in more places than this; a longer leg is read
/// in longer steps.
const GROUND_PLACES_AT_MOST: usize = 2000;

/// A wind split along a course.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CourseWind {
    /// The part that blows along the course, m/s: positive behind the
    /// glider (a tail wind), negative against it (a head wind).
    pub along_ms: f64,
    /// The part that blows across it, m/s: positive from the left, which
    /// pushes the glider to the right, negative from the right.
    pub across_ms: f64,
}

/// The wind split into its parts along and across a course, degrees
/// clockwise from north.
#[must_use]
pub fn wind_on_course(course_deg: f64, wind: Wind) -> CourseWind {
    // The wind blows towards the opposite of where it comes from.
    let off_course = (wind.from_deg + 180.0 - course_deg).to_radians();
    CourseWind {
        along_ms: wind.speed_ms * off_course.cos(),
        across_ms: wind.speed_ms * off_course.sin(),
    }
}

/// The mean of two winds as arrows: two that blow against each other make
/// less than either.
#[must_use]
pub fn mean_wind(one: Wind, other: Wind) -> Wind {
    let parts = |wind: Wind| {
        let towards = (wind.from_deg + 180.0).to_radians();
        (wind.speed_ms * towards.sin(), wind.speed_ms * towards.cos())
    };
    let (east_a, north_a) = parts(one);
    let (east_b, north_b) = parts(other);
    let (east, north) = ((east_a + east_b) / 2.0, (north_a + north_b) / 2.0);
    let speed_ms = east.hypot(north);
    let from_deg = if speed_ms < 1e-9 {
        one.from_deg
    } else {
        (east.atan2(north).to_degrees() + 180.0).rem_euclid(360.0)
    };
    Wind { from_deg, speed_ms }
}

/// When the pilot is at each point of a route, seconds after leaving the
/// first: one entry per point, the first zero. `legs_m` are the legs'
/// lengths in order. `None` for a speed that is no speed.
#[must_use]
pub fn times_at_points(legs_m: &[f64], ground_speed_ms: f64) -> Option<Vec<f64>> {
    if !ground_speed_ms.is_finite() || ground_speed_ms <= 0.0 {
        return None;
    }
    let mut along = 0.0;
    let mut times = Vec::with_capacity(legs_m.len() + 1);
    times.push(0.0);
    for leg in legs_m {
        along += leg.max(0.0);
        times.push(along / ground_speed_ms);
    }
    Some(times)
}

/// The height to leave a point at, for one leg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Departure {
    /// A height that reaches the next point.
    Height {
        /// The height to leave at, metres above mean sea level.
        altitude_m: f64,
        /// The glide over the ground in the leg's wind, metres forward per
        /// metre of height.
        glide: f64,
        /// The ground that decides the height: where along the leg it is,
        /// metres from its start, and how high, metres.
        deciding_at_m: f64,
        /// The height of that ground.
        deciding_ground_m: f64,
        /// The share of the places along the line whose ground is held,
        /// from 0 to 1. Under 1 the height may be too low.
        ground_held: f64,
    },
    /// The wind lets the glider make no way along the leg.
    NoWay,
    /// No ground is held along the leg.
    NoGround,
}

/// The height to leave `from` at to reach `to` in a straight glide with
/// `margin_m` over the ground everywhere on the way, the ground at `to`
/// included, at the glider's trim speed in the wind of the leg.
#[must_use]
pub fn departure_height(
    terrain: &Terrain,
    from: GeoPoint,
    to: GeoPoint,
    wind: Option<Wind>,
    glider: &Glider,
    margin_m: f64,
) -> Departure {
    let course = bearing_deg(from, to);
    let Some(glide) = glide_over_ground(course, wind, glider) else {
        return Departure::NoWay;
    };
    let length_m = from.distance_m(to);
    #[allow(clippy::cast_precision_loss)]
    let step_m = GROUND_STEP_M.max(length_m / GROUND_PLACES_AT_MOST as f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = (length_m / step_m).floor() as usize + 1;
    #[allow(clippy::cast_precision_loss)]
    let mut places: Vec<(f64, Option<f64>)> = terrain
        .profile(from, course, step_m, count)
        .into_iter()
        .enumerate()
        .map(|(index, ground)| (index as f64 * step_m, ground))
        // The ground at the start is where the glide begins, not ground
        // to keep a margin over: a take-off is left from its slope.
        .skip(1)
        .collect();
    places.push((length_m, terrain.elevation_m(to)));

    let held: Vec<(f64, f64)> = places
        .iter()
        .filter_map(|(at, ground)| ground.map(|ground| (*at, ground)))
        .collect();
    // The height at the start that keeps the margin over a place is its
    // ground, the margin, and what the glide loses on the way there.
    let deciding = held
        .iter()
        .map(|(at, ground)| (*at, *ground, ground + margin_m + at / glide))
        .max_by(|a, b| a.2.total_cmp(&b.2));
    let Some((deciding_at_m, deciding_ground_m, altitude_m)) = deciding else {
        return Departure::NoGround;
    };
    #[allow(clippy::cast_precision_loss)]
    let ground_held = held.len() as f64 / places.len() as f64;
    Departure::Height {
        altitude_m,
        glide,
        deciding_at_m,
        deciding_ground_m,
        ground_held,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TileAddress;

    fn place(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    fn close(found: f64, expected: f64, within: f64) {
        assert!(
            (found - expected).abs() <= within,
            "{found} is not {expected}"
        );
    }

    #[test]
    fn a_wind_is_split_along_and_across_a_course() {
        // Flying east into a wind from the east: all head wind.
        let head = wind_on_course(
            90.0,
            Wind {
                from_deg: 90.0,
                speed_ms: 5.0,
            },
        );
        close(head.along_ms, -5.0, 1e-9);
        close(head.across_ms, 0.0, 1e-9);
        // From the west: all tail wind.
        let tail = wind_on_course(
            90.0,
            Wind {
                from_deg: 270.0,
                speed_ms: 5.0,
            },
        );
        close(tail.along_ms, 5.0, 1e-9);
        // Flying east, a wind from the north comes from the left.
        let left = wind_on_course(
            90.0,
            Wind {
                from_deg: 0.0,
                speed_ms: 5.0,
            },
        );
        close(left.along_ms, 0.0, 1e-9);
        close(left.across_ms, 5.0, 1e-9);
        let right = wind_on_course(
            90.0,
            Wind {
                from_deg: 180.0,
                speed_ms: 5.0,
            },
        );
        close(right.across_ms, -5.0, 1e-9);
    }

    #[test]
    fn two_winds_are_averaged_as_arrows() {
        let west = Wind {
            from_deg: 270.0,
            speed_ms: 4.0,
        };
        let north = Wind {
            from_deg: 0.0,
            speed_ms: 4.0,
        };
        let mean = mean_wind(west, north);
        close(mean.from_deg, 315.0, 1e-9);
        close(mean.speed_ms, 8f64.sqrt(), 1e-9);
        // Across north.
        let mean = mean_wind(
            Wind {
                from_deg: 350.0,
                speed_ms: 4.0,
            },
            Wind {
                from_deg: 10.0,
                speed_ms: 4.0,
            },
        );
        assert!(mean.from_deg < 0.01 || mean.from_deg > 359.99, "{mean:?}");
        // Against each other: nothing.
        let calm = mean_wind(
            west,
            Wind {
                from_deg: 90.0,
                speed_ms: 4.0,
            },
        );
        close(calm.speed_ms, 0.0, 1e-9);
    }

    #[test]
    fn the_time_at_a_point_is_the_way_to_it_over_the_speed() {
        // Two legs of 10 and 15 km at 25 km/h.
        let times = times_at_points(&[10_000.0, 15_000.0], 25.0 / 3.6).unwrap();
        assert_eq!(times.len(), 3);
        close(times[0], 0.0, 1e-9);
        close(times[1], 1440.0, 1e-6);
        close(times[2], 3600.0, 1e-6);
        assert_eq!(times_at_points(&[], 7.0), Some(vec![0.0]));
        assert_eq!(times_at_points(&[1000.0], 0.0), None);
        assert_eq!(times_at_points(&[1000.0], f64::NAN), None);
    }

    /// A terrain of one tile of zoom 12 around the Wallberg: flat at
    /// 800 m, with a ridge of 1600 m along the columns from 200 to 210.
    fn terrain() -> (Terrain, GeoPoint, GeoPoint) {
        let side = 512_u32;
        let mut rgba = Vec::new();
        for _row in 0..side {
            for column in 0..side {
                let height: f64 = if (200..=210).contains(&column) {
                    1600.0
                } else {
                    800.0
                };
                let coded = height + 32_768.0;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                rgba.extend([(coded / 256.0).floor() as u8, (coded % 256.0) as u8, 0, 255]);
            }
        }
        let wallberg = place(47.6642, 11.7967);
        let address = TileAddress::holding(wallberg, 12).unwrap();
        let mut terrain = Terrain::new();
        terrain.add_pixels(address, side, &rgba).unwrap();
        // Two places on the tile, west and east of the ridge, on one line
        // of latitude.
        let (west, east) = (place(47.6642, 11.785), place(47.6642, 11.86));
        (terrain, west, east)
    }

    #[test]
    fn a_point_is_left_high_enough_to_clear_the_ground_on_the_way() {
        let (terrain, west, east) = terrain();
        let glider = Glider {
            trim_speed_ms: 10.0,
            glide_ratio: 8.0,
        };
        let length = west.distance_m(east);
        let Departure::Height {
            altitude_m,
            glide,
            deciding_ground_m,
            deciding_at_m,
            ground_held,
        } = departure_height(&terrain, west, east, None, &glider, 200.0)
        else {
            panic!("a height");
        };
        close(glide, 8.0, 1e-9);
        close(ground_held, 1.0, 1e-9);
        // The ridge, some two kilometres along, decides: 200 m over it
        // after a glide of 2 km at 8 needs more than 200 m over the far
        // end after one of 5.6 km.
        close(deciding_ground_m, 1600.0, 1e-9);
        assert!(
            deciding_at_m > 1500.0 && deciding_at_m < 2500.0,
            "{deciding_at_m}"
        );
        close(altitude_m, 1600.0 + 200.0 + deciding_at_m / 8.0, 1e-6);
        assert!(altitude_m > 800.0 + 200.0 + length / 8.0);
        // A head wind takes glide away and asks for more height.
        let Departure::Height {
            altitude_m: into_wind,
            glide: worse,
            ..
        } = departure_height(
            &terrain,
            west,
            east,
            Some(Wind {
                from_deg: 90.0,
                speed_ms: 4.0,
            }),
            &glider,
            200.0,
        )
        else {
            panic!("a height");
        };
        close(worse, 6.0 / 10.0 * 8.0, 1e-5);
        assert!(into_wind > altitude_m);
        // A wind stronger than the glider flies makes no way.
        assert_eq!(
            departure_height(
                &terrain,
                west,
                east,
                Some(Wind {
                    from_deg: 90.0,
                    speed_ms: 12.0,
                }),
                &glider,
                200.0,
            ),
            Departure::NoWay
        );
        // Without the ground there is nothing to say.
        assert_eq!(
            departure_height(&Terrain::new(), west, east, None, &glider, 200.0),
            Departure::NoGround
        );
    }
}
