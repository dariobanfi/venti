//! The layering of the air over a place, from the temperatures a weather
//! model has at its levels.
//!
//! Rising air that forms no cloud cools by almost one degree for every
//! hundred metres. Where the air around it cools by much less than that
//! with height, or grows warmer, a thermal is soon no warmer than what it
//! rises through: there thermals stop. [`layers`] finds those stretches
//! between the model's levels.
//!
//! It is coarse, and says so wherever it is shown. A series has the
//! temperature 2 m above the model's ground, at three levels within 180 m
//! of it, and on surfaces of equal pressure some 1,500 m apart. Between
//! two levels only the mean is known: an inversion a hundred metres thick
//! at 2,200 m is, between the surfaces at 1,500 and 3,000 m, a slightly
//! slower fall of the whole stretch, or nothing. A true sounding needs the
//! model's own levels, which no source read today gives.
//!
//! The limits are our own, the owner's to change, and to verify against
//! soundings.

use venti_domain::ForecastHour;

/// The fall of temperature with height below which the air between two
/// levels counts as stable, °C per 100 m. Air that forms cloud as it rises
/// cools by about this much in the lower Alps' summer air, so air that
/// cools more slowly stops rising air with or without cloud. Ours, and the
/// owner's to change; to verify.
pub const STABLE_BELOW_C_PER_100M: f64 = 0.5;
/// How far above the last level read the next has to lie to be read,
/// metres. The model's temperatures come to a tenth of a degree, and over
/// less height than this a tenth would decide. The three levels near the
/// ground therefore make one step, from 2 m to 180 m above it.
pub const LEVELS_APART_M: f64 = 150.0;
/// The temperature at the ground is the model's for 2 m above it.
const SCREEN_HEIGHT_M: f64 = 2.0;

/// What a stretch of air does to a thermal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stability {
    /// The temperature falls with height, by less than
    /// [`STABLE_BELOW_C_PER_100M`], or stays the same.
    Stable,
    /// The temperature rises with height.
    Inversion,
}

/// A stretch of air between two of the model's levels in which thermals
/// stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    /// Its lower level, metres above mean sea level.
    pub bottom_m: f64,
    /// Its upper level.
    pub top_m: f64,
    /// Whether the temperature falls too slowly in it, or rises.
    pub kind: Stability,
    /// By how much the temperature falls with height between the two
    /// levels, °C per 100 m: the mean over the stretch. Negative in an
    /// inversion.
    pub fall_c_per_100m: f64,
}

/// The stable stretches and the inversions between the levels of an hour
/// of a model whose ground lies at `ground_m` above mean sea level, lowest
/// first.
///
/// The levels read are the temperature 2 m above the ground and then,
/// going up, each level that lies at least [`LEVELS_APART_M`] above the
/// last one read. Stretches of the same kind that follow each other are
/// one layer, with the fall over the whole of it. Empty when the hour has
/// fewer than two such temperatures, and when the air is unstable all the
/// way up.
#[must_use]
pub fn layers(hour: &ForecastHour, ground_m: f64) -> Vec<Layer> {
    let foot = ground_m + SCREEN_HEIGHT_M;
    let mut above: Vec<(f64, f64)> = hour
        .winds_aloft
        .iter()
        .filter_map(|level| {
            let height = match level.above_ground_m {
                Some(above) => ground_m + above,
                None => level.altitude_m?,
            };
            Some((height, level.temperature_c?))
        })
        .filter(|(height, _)| *height > foot)
        .collect();
    above.sort_by(|a, b| a.0.total_cmp(&b.0));

    // The levels read: the ground's, where the hour has it, and each that
    // lies far enough above the last.
    let mut read: Vec<(f64, f64)> = hour
        .temperature_c
        .map(|warmth| (foot, warmth))
        .into_iter()
        .collect();
    for level in above {
        if read
            .last()
            .is_none_or(|(last, _)| level.0 - last >= LEVELS_APART_M)
        {
            read.push(level);
        }
    }
    let mut found: Vec<Layer> = Vec::new();
    for pair in read.windows(2) {
        let ((bottom_m, below), (top_m, above)) = (pair[0], pair[1]);
        let fall = (below - above) / (top_m - bottom_m) * 100.0;
        let kind = if fall < 0.0 {
            Stability::Inversion
        } else if fall < STABLE_BELOW_C_PER_100M {
            Stability::Stable
        } else {
            continue;
        };
        match found.last_mut() {
            Some(last) if last.kind == kind && last.top_m == bottom_m => {
                // One layer with the stretch below it: the fall over both.
                let fallen =
                    last.fall_c_per_100m * (last.top_m - last.bottom_m) + fall * (top_m - bottom_m);
                last.top_m = top_m;
                last.fall_c_per_100m = fallen / (top_m - last.bottom_m);
            }
            _ => found.push(Layer {
                bottom_m,
                top_m,
                kind,
                fall_c_per_100m: fall,
            }),
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::WindAloft;

    use super::*;

    /// An hour over ground at 1000 m with these temperatures: at 2 m, at
    /// 80, 120 and 180 m above the ground, and on surfaces at 1500, 3000
    /// and 4200 m.
    fn hour(ground: Option<f64>, near: [f64; 3], surfaces: [f64; 3]) -> ForecastHour {
        let mut hour = ForecastHour::empty(Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap());
        hour.temperature_c = ground;
        let level = |above: Option<f64>, altitude: Option<f64>, warmth: f64| WindAloft {
            above_ground_m: above,
            pressure_hpa: altitude.map(|_| 850.0),
            altitude_m: altitude,
            wind_speed_ms: None,
            wind_direction_deg: None,
            temperature_c: Some(warmth),
        };
        hour.winds_aloft = [80.0, 120.0, 180.0]
            .into_iter()
            .zip(near)
            .map(|(above, warmth)| level(Some(above), None, warmth))
            .chain(
                [1500.0, 3000.0, 4200.0]
                    .into_iter()
                    .zip(surfaces)
                    .map(|(altitude, warmth)| level(None, Some(altitude), warmth)),
            )
            .collect();
        hour
    }

    #[test]
    fn air_that_cools_fast_enough_all_the_way_up_has_no_layer() {
        // 0.9 °C per 100 m near the ground, 0.7 above.
        let found = layers(
            &hour(Some(20.0), [19.3, 18.9, 18.4], [16.2, 5.7, -2.7]),
            1000.0,
        );
        assert_eq!(found, []);
    }

    #[test]
    fn a_slow_fall_between_two_surfaces_is_a_stable_layer() {
        // From 1500 to 3000 m the air cools by 4.5 °C: 0.3 °C per 100 m.
        let found = layers(
            &hour(Some(20.0), [19.3, 18.9, 18.4], [16.2, 11.7, 3.3]),
            1000.0,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].bottom_m, found[0].top_m), (1500.0, 3000.0));
        assert_eq!(found[0].kind, Stability::Stable);
        assert!((found[0].fall_c_per_100m - 0.3).abs() < 1e-9);
    }

    #[test]
    fn a_cold_morning_at_the_ground_is_an_inversion_of_one_step_to_180_m() {
        // 6 °C at 2 m and 9.5 °C at 180 m above the ground: the levels at
        // 80 and 120 m are not read by themselves.
        let found = layers(
            &hour(Some(6.0), [8.0, 9.0, 9.5], [8.0, -2.0, -10.0]),
            1000.0,
        );
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!((found[0].bottom_m, found[0].top_m), (1002.0, 1180.0));
        assert_eq!(found[0].kind, Stability::Inversion);
        assert!((found[0].fall_c_per_100m + 3.5 / 1.78).abs() < 1e-9);
        // Above it, to the surface at 1500 m, 1.5 °C in 320 m: stable.
        assert_eq!((found[1].bottom_m, found[1].top_m), (1180.0, 1500.0));
        assert_eq!(found[1].kind, Stability::Stable);
    }

    #[test]
    fn stretches_of_one_kind_that_follow_each_other_are_one_layer() {
        // 0.4 °C per 100 m from 1500 to 3000 m and 0.25 from there to
        // 4200 m: 9 °C in 2700 m.
        let found = layers(
            &hour(Some(20.0), [19.3, 18.9, 18.4], [16.2, 10.2, 7.2]),
            1000.0,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].bottom_m, found[0].top_m), (1500.0, 4200.0));
        assert!((found[0].fall_c_per_100m - 9.0 / 27.0).abs() < 1e-9);
    }

    #[test]
    fn a_surface_under_the_ground_or_just_above_a_level_is_not_read() {
        // Ground at 1400 m: the levels near it reach 1580 m, and the
        // surface at 1500 m lies among them.
        let found = layers(
            &hour(Some(15.0), [14.3, 13.9, 13.4], [30.0, 2.0, -6.0]),
            1400.0,
        );
        assert_eq!(found, []);
        // Without a temperature at the ground the lowest level is the
        // first read, and an hour with one temperature has nothing to say.
        let mut bare = hour(None, [8.0, 9.0, 9.5], [9.0, -2.0, -10.0]);
        let found = layers(&bare, 1000.0);
        assert_eq!((found[0].bottom_m, found[0].top_m), (1080.0, 1500.0));
        assert_eq!(found[0].kind, Stability::Inversion);
        bare.winds_aloft.truncate(1);
        assert_eq!(layers(&bare, 1000.0), []);
    }
}
