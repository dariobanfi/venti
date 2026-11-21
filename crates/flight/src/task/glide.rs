//! The glide along the rest of a task: how much height the shortest way
//! through the remaining cylinders costs, leg by leg, at the glider's trim
//! speed in the wind as last worked out.
//!
//! This is the arithmetic of a final glide, as an instrument's competition
//! page shows it for the next turnpoint and for goal. Each leg is flown in
//! a straight line along the shortest way ([`super::route`]), holding its
//! course against the wind; the height a leg costs is its length over the
//! glide ratio made good over the ground on it
//! ([`crate::live::navigation::glide_over_ground`]). The air neither rises
//! nor sinks on the way, and the wind is the same everywhere: advice, not
//! a promise.

use venti_domain::GeoPoint;

use crate::geodesy::Earth;
use crate::live::Wind;
use crate::live::navigation::Glides;

/// How far it is to a point of the way, and what getting there costs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlideTo {
    /// The distance along the way, metres.
    pub distance_m: f64,
    /// The height the glide loses on the way there, metres.
    pub height_lost_m: f64,
    /// How long the glide takes there, seconds, at trim speed in the wind.
    pub time_s: f64,
}

/// The height kept back for every turnpoint still to round before a
/// place, metres: rounding a cylinder costs a turn, and a line through
/// its edge is never flown exactly. XCTrack's manual, as its makers'
/// partner publishes it, takes a fixed 15 m for each (read 2026-11-21
/// through a condensing reader: *to check against the page itself*).
pub const TURNPOINT_ALLOWANCE_M: f64 = 15.0;

/// The height kept back on the way to a place with `turnpoints_before`
/// turnpoints still to round before it, metres.
pub fn allowance_m(turnpoints_before: usize) -> f64 {
    TURNPOINT_ALLOWANCE_M * turnpoints_before as f64
}

/// Which speed a time of arrival is worked out from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrivalSpeed {
    /// The speed made along the task so far: the speed section's average,
    /// and before there is one the speed over the ground.
    #[default]
    Average,
    /// The speed over the ground now.
    Current,
    /// The glider's own speed in the wind along the shortest way: trim
    /// speed, or with a polar the speed that carries furthest.
    TrimInWind,
}

/// How long the way to a place takes at the speed chosen, seconds.
/// `in_wind_s` is the time of [`along_way`] to the place. `None` when
/// the speed chosen is not known or under a walking pace.
pub fn time_to_s(
    choice: ArrivalSpeed,
    distance_m: f64,
    average_ms: Option<f64>,
    current_ms: Option<f64>,
    in_wind_s: Option<f64>,
) -> Option<f64> {
    let at = |speed: Option<f64>| {
        speed
            .filter(|speed| *speed > 1.0)
            .map(|speed| distance_m / speed)
    };
    match choice {
        ArrivalSpeed::Average => at(average_ms.or(current_ms)),
        ArrivalSpeed::Current => at(current_ms),
        ArrivalSpeed::TrimInWind => in_wind_s,
    }
}

/// Works out the glide from `from` along `way`, one entry for each point
/// of the way in its order: the distance, height and time to it, counted
/// from `from`. `None` when the wind lets the glider make no way on one
/// of the legs: then nothing after it can be said either.
pub fn along_way(
    earth: Earth,
    from: GeoPoint,
    way: &[GeoPoint],
    wind: Option<Wind>,
    glider: &impl Glides,
) -> Option<Vec<GlideTo>> {
    let mut sum = GlideTo {
        distance_m: 0.0,
        height_lost_m: 0.0,
        time_s: 0.0,
    };
    let mut last = from;
    let mut out = Vec::with_capacity(way.len());
    for point in way {
        let course = earth.course(last, *point);
        // A leg of no length costs nothing, whatever the wind.
        if course.distance_m > 0.5 {
            let made = glider.on_course(course.bearing_deg, wind)?;
            sum.distance_m += course.distance_m;
            sum.height_lost_m += course.distance_m / made.glide();
            sum.time_s += course.distance_m / made.ground_speed_ms;
        }
        out.push(sum);
        last = *point;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::navigation::Glider;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    const GLIDER: Glider = Glider {
        trim_speed_ms: 10.0,
        glide_ratio: 8.0,
    };

    /// One degree of longitude at 47 degrees north, metres, on the sphere.
    const DEG_LON_M: f64 = 75_826.0;

    #[test]
    fn in_still_air_each_leg_costs_its_length_over_the_glide() {
        let from = point(47.0, 11.0);
        // 8 km east, then 8 km north.
        let east = point(47.0, 11.0 + 8000.0 / DEG_LON_M);
        let north = point(47.0 + 8000.0 / 111_195.0, east.lon());
        let glide = along_way(Earth::FaiSphere, from, &[east, north], None, &GLIDER).unwrap();

        assert_eq!(glide.len(), 2);
        assert!((glide[0].distance_m - 8000.0).abs() < 10.0, "{glide:?}");
        assert!((glide[0].height_lost_m - 1000.0).abs() < 2.0, "{glide:?}");
        assert!((glide[0].time_s - 800.0).abs() < 2.0, "{glide:?}");
        // The second entry counts from the start: both legs.
        assert!((glide[1].distance_m - 16_000.0).abs() < 20.0, "{glide:?}");
        assert!((glide[1].height_lost_m - 2000.0).abs() < 4.0, "{glide:?}");
    }

    #[test]
    fn the_wind_costs_on_the_leg_into_it_and_gives_on_the_leg_with_it() {
        let from = point(47.0, 11.0);
        let east = point(47.0, 11.0 + 8000.0 / DEG_LON_M);
        // 5 m/s from the east: half the speed over the ground out, half as
        // much again back.
        let wind = Some(Wind {
            from_deg: 90.0,
            speed_ms: 5.0,
        });
        let glide = along_way(Earth::FaiSphere, from, &[east, from], wind, &GLIDER).unwrap();
        assert!((glide[0].height_lost_m - 2000.0).abs() < 5.0, "{glide:?}");
        assert!(
            (glide[1].height_lost_m - (2000.0 + 8000.0 / 12.0)).abs() < 6.0,
            "{glide:?}"
        );
        assert!((glide[0].time_s - 1600.0).abs() < 4.0, "{glide:?}");
    }

    #[test]
    fn a_leg_the_wind_forbids_leaves_the_glide_unknown() {
        let from = point(47.0, 11.0);
        let east = point(47.0, 11.0 + 8000.0 / DEG_LON_M);
        let gale = Some(Wind {
            from_deg: 90.0,
            speed_ms: 12.0,
        });
        assert_eq!(
            along_way(Earth::FaiSphere, from, &[east], gale, &GLIDER),
            None
        );
        // No way at all is no glide, and costs nothing.
        assert_eq!(
            along_way(Earth::FaiSphere, from, &[], gale, &GLIDER),
            Some(Vec::new())
        );
        // Standing on the point: nothing to fly, whatever the wind.
        let here = along_way(Earth::FaiSphere, from, &[from], gale, &GLIDER).unwrap();
        assert_eq!(here[0].height_lost_m, 0.0);
    }

    #[test]
    fn an_arrival_time_follows_the_speed_chosen_and_each_turnpoint_keeps_height_back() {
        // 12 km: at an average of 8 m/s, at 12 m/s now, in 20 minutes at
        // trim in the wind.
        let time = |choice| time_to_s(choice, 12_000.0, Some(8.0), Some(12.0), Some(1_200.0));
        assert_eq!(time(ArrivalSpeed::Average), Some(1_500.0));
        assert_eq!(time(ArrivalSpeed::Current), Some(1_000.0));
        assert_eq!(time(ArrivalSpeed::TrimInWind), Some(1_200.0));
        // No average yet: the speed now. Standing still: no time.
        assert_eq!(
            time_to_s(ArrivalSpeed::Average, 12_000.0, None, Some(12.0), None),
            Some(1_000.0)
        );
        assert_eq!(
            time_to_s(ArrivalSpeed::Current, 12_000.0, Some(8.0), Some(0.4), None),
            None
        );
        assert_eq!(allowance_m(0), 0.0);
        assert_eq!(allowance_m(3), 45.0);
    }
}
