//! Thermals, from an hour of a weather model.
//!
//! A model with cells two kilometres wide computes no thermals: they are
//! a few hundred metres across. What it does compute is what makes them:
//! how much heat the ground hands to the air, how warm the air is at the
//! ground and higher up, and how moist. From those, three figures are
//! worked out here for the model's cell, the way soaring forecasts have
//! long done it:
//!
//! - **How high thermals reach.** Air that leaves the ground at the
//!   ground's temperature cools by 9.8 °C for every kilometre it rises.
//!   It keeps rising while it is warmer than the air around it, which the
//!   model gives at its levels; where it no longer is, thermals end. This
//!   is the parcel method of every sounding diagram.
//! - **Whether cloud caps them.** Rising air condenses about 125 m above
//!   the ground for every degree between temperature and dew point there
//!   (Espy's rule). If that is lower than the dry thermals reach, cumulus
//!   forms and its base is the ceiling. Where the model itself has
//!   convective cloud, its base is taken instead: the model knows its own
//!   moisture better than a rule does.
//! - **How strong they are.** The convective velocity scale of Deardorff
//!   (1970), `w* = (g / T · H / (ρ · cp) · depth)^(1/3)`: the more heat `H`
//!   the ground gives and the deeper the layer it stirs, the faster the
//!   air in thermals rises. It is what RASP's "thermal updraft velocity"
//!   is. A glider climbs by that less its own sink.
//!
//! What this is not: a forecast of any one thermal, slope or valley. The
//! model's ground is smoothed, its levels are far apart (between 1500 m and
//! 3000 m there is none, so an inversion in between is seen only as a
//! weaker average), and the scale was found over flat land; in the
//! mountains, sunlit slopes gather the heat and climbs are often stronger.
//! The glider's sink below is a round figure. To verify, all of it, against
//! climbs that were flown.

use venti_domain::ForecastHour;

/// How fast rising air cools while no cloud forms in it, °C per metre.
const DRY_LAPSE_C_PER_M: f64 = 0.0098;
/// How high above the ground rising air condenses, metres for every
/// degree between the temperature and the dew point there.
const CONDENSATION_M_PER_C: f64 = 125.0;
/// Gravity, m/s².
const GRAVITY_MS2: f64 = 9.81;
/// The heat a kilogram of air takes to warm by one degree, J/(kg·K).
const HEAT_CAPACITY_J_KG_K: f64 = 1005.0;
/// The gas constant of dry air, J/(kg·K).
const GAS_CONSTANT_J_KG_K: f64 = 287.05;
/// What a paraglider sinks through the air while it circles, m/s. A round
/// figure: wings and pilots differ. To verify against flown climbs.
pub const GLIDER_SINK_MS: f64 = 1.0;
/// The temperature at the ground is the model's for 2 m above it.
const SCREEN_HEIGHT_M: f64 = 2.0;

/// What an hour of a forecast says about thermals over the model's cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Convection {
    /// How high a glider can climb in them, metres above mean sea level:
    /// the cloud base where cumulus caps the thermals, else the height at
    /// which rising air is no longer warmer than the air around it. The
    /// height of the model's ground when there are no thermals.
    pub top_m: f64,
    /// The base of the cumulus on the thermals, metres above mean sea
    /// level. `None` on a blue day, and when there are no thermals.
    pub cloud_base_m: Option<f64>,
    /// How deep the layer is that the thermals stir, metres above the
    /// model's ground. Zero when the ground does not warm the air, or the
    /// air is stable from the ground up.
    pub depth_m: f64,
    /// How fast the air in thermals rises, m/s: the convective velocity
    /// scale. Zero without thermals.
    pub updraft_ms: f64,
    /// The climb to expect of a paraglider in them, m/s: the updraft less
    /// [`GLIDER_SINK_MS`], and never below zero.
    pub climb_ms: f64,
    /// The strongest wind the model has in that layer, from 10 m above
    /// its ground up to `top_m`, m/s. `None` when the model gives no wind.
    pub wind_ms: Option<f64>,
}

/// Works out the thermals of one hour over a model's cell whose ground
/// lies at `ground_m` above mean sea level.
///
/// `None` when the hour lacks what the figures are made from: the heat
/// the ground gives to the air, the temperature at the ground, or a
/// temperature at any level above it. An hour without thermals is not
/// `None`: it has a depth and a climb of zero.
pub fn convection(hour: &ForecastHour, ground_m: f64) -> Option<Convection> {
    let heat = hour.heat_to_air_w_m2?;
    let warmth = hour.temperature_c?;
    let foot = ground_m + SCREEN_HEIGHT_M;

    // The air above the ground: each level's height above the sea and its
    // temperature, lowest first.
    let mut air: Vec<(f64, f64)> = hour
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
    air.sort_by(|a, b| a.0.total_cmp(&b.0));
    if air.is_empty() {
        return None;
    }

    let none = Convection {
        top_m: ground_m,
        cloud_base_m: None,
        depth_m: 0.0,
        updraft_ms: 0.0,
        climb_ms: 0.0,
        wind_ms: wind_up_to(hour, ground_m, ground_m),
    };
    // Without heat from the ground nothing rises, however cold it is above.
    if heat <= 0.0 {
        return Some(none);
    }

    // How much warmer than its surroundings air from the ground is at a
    // height, having cooled on the way up.
    let excess =
        |(height, around): (f64, f64)| warmth - DRY_LAPSE_C_PER_M * (height - foot) - around;
    // Stable from the ground up: the first level is as warm as air risen
    // to it, or warmer.
    if excess(air[0]) <= 0.0 {
        return Some(none);
    }
    // Where the excess is used up, between the last level it still had
    // one at and the first it has none. Air that is still warmer at the
    // model's highest level read is taken to end there.
    let mut dry_top = air[air.len() - 1].0;
    for pair in air.windows(2) {
        let (below, above) = (excess(pair[0]), excess(pair[1]));
        if above <= 0.0 {
            dry_top = pair[0].0 + (pair[1].0 - pair[0].0) * below / (below - above);
            break;
        }
    }

    // Cumulus: the model's own base where it has convective cloud, else
    // where the rule says rising air condenses, if thermals get that far.
    let condensation = hour
        .dew_point_c
        .map(|dew| foot + CONDENSATION_M_PER_C * (warmth - dew).max(0.0));
    let cloud_base_m = hour
        .convective_cloud_base_m
        .filter(|base| *base > foot)
        .or(condensation.filter(|level| *level < dry_top));
    let top_m = cloud_base_m.unwrap_or(dry_top);
    let depth_m = (top_m - ground_m).max(0.0);

    // The air's density at the ground, from the standard atmosphere's
    // pressure at that height and the temperature there.
    let kelvin = warmth + 273.15;
    let pressure_pa = 101_325.0 * (1.0 - 2.255_77e-5 * ground_m).max(0.1).powf(5.255_88);
    let density = pressure_pa / (GAS_CONSTANT_J_KG_K * kelvin);
    let updraft_ms =
        (GRAVITY_MS2 / kelvin * heat / (density * HEAT_CAPACITY_J_KG_K) * depth_m).cbrt();

    Some(Convection {
        top_m,
        cloud_base_m,
        depth_m,
        updraft_ms,
        climb_ms: (updraft_ms - GLIDER_SINK_MS).max(0.0),
        wind_ms: wind_up_to(hour, ground_m, top_m),
    })
}

/// The strongest wind the model has from 10 m above its ground up to a
/// height above the sea.
fn wind_up_to(hour: &ForecastHour, ground_m: f64, top_m: f64) -> Option<f64> {
    let aloft = hour.winds_aloft.iter().filter_map(|level| {
        let height = match level.above_ground_m {
            Some(above) => ground_m + above,
            None => level.altitude_m?,
        };
        (height > ground_m && height <= top_m)
            .then_some(level.wind_speed_ms)
            .flatten()
    });
    hour.wind_speed_ms
        .into_iter()
        .chain(aloft)
        .max_by(f64::total_cmp)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::WindAloft;

    use super::*;

    /// A level at a height above the ground with its temperature and wind.
    fn above(metres: f64, temperature_c: f64, wind_ms: f64) -> WindAloft {
        WindAloft {
            temperature_c: Some(temperature_c),
            wind_speed_ms: Some(wind_ms),
            ..WindAloft::above_ground(metres)
        }
    }

    /// A pressure surface at a height above the sea.
    fn surface(hpa: f64, altitude_m: f64, temperature_c: f64, wind_ms: f64) -> WindAloft {
        WindAloft {
            altitude_m: Some(altitude_m),
            temperature_c: Some(temperature_c),
            wind_speed_ms: Some(wind_ms),
            ..WindAloft::at_pressure(hpa)
        }
    }

    /// A summer afternoon over a cell whose ground lies at 1000 m: 24 °C at
    /// the ground under the sun, well mixed to above 1500 m, and a layer
    /// between there and 3100 m that cools by 8.3 °C per kilometre.
    fn afternoon() -> ForecastHour {
        let mut hour = ForecastHour::empty(Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap());
        hour.temperature_c = Some(24.0);
        hour.dew_point_c = Some(8.0);
        hour.heat_to_air_w_m2 = Some(280.0);
        hour.wind_speed_ms = Some(2.0);
        hour.winds_aloft = vec![
            above(80.0, 22.0, 3.0),
            above(120.0, 21.6, 3.0),
            above(180.0, 21.0, 3.5),
            surface(850.0, 1500.0, 17.8, 4.0),
            surface(700.0, 3100.0, 4.5, 9.0),
            surface(600.0, 4300.0, -3.0, 12.0),
        ];
        hour
    }

    const GROUND_M: f64 = 1000.0;

    #[test]
    fn a_summer_afternoon_has_blue_thermals_to_where_the_air_is_no_longer_colder() {
        let thermals = convection(&afternoon(), GROUND_M).unwrap();

        // Air from the ground is 1.3 °C warmer than the air at 1500 m and
        // 1.1 °C colder than the air at 3100 m: it stops a little over
        // half way between.
        assert!((thermals.top_m - 2390.0).abs() < 15.0, "{thermals:?}");
        assert!((thermals.depth_m - (thermals.top_m - GROUND_M)).abs() < 1e-9);
        // Sixteen degrees between temperature and dew point put the
        // condensation level at 3000 m, above the thermals: a blue day.
        assert_eq!(thermals.cloud_base_m, None);
        // 280 W/m² into a layer 1.4 km deep: the air in thermals rises at
        // 2.3 m/s, a paraglider in it at 1.3.
        assert!((thermals.updraft_ms - 2.3).abs() < 0.05, "{thermals:?}");
        assert!((thermals.climb_ms - (thermals.updraft_ms - GLIDER_SINK_MS)).abs() < 1e-9);
        // The strongest wind in that layer is the one at 1500 m; the wind
        // at 3100 m blows above the thermals.
        assert_eq!(thermals.wind_ms, Some(4.0));
    }

    #[test]
    fn moist_air_caps_the_thermals_with_cumulus_at_the_condensation_level() {
        let mut hour = afternoon();
        hour.dew_point_c = Some(16.0);

        let thermals = convection(&hour, GROUND_M).unwrap();

        // Eight degrees: 1000 m above the ground's 2 m.
        assert_eq!(thermals.cloud_base_m, Some(2002.0));
        assert_eq!(thermals.top_m, 2002.0);
        // A shallower layer than the blue day's: weaker thermals.
        let blue = convection(&afternoon(), GROUND_M).unwrap();
        assert!(thermals.updraft_ms < blue.updraft_ms);
    }

    #[test]
    fn where_the_model_has_convective_cloud_its_base_is_the_ceiling() {
        let mut hour = afternoon();
        hour.convective_cloud_base_m = Some(2650.0);

        let thermals = convection(&hour, GROUND_M).unwrap();

        // Higher than the parcel alone gets: the model's own base stands.
        assert_eq!(thermals.cloud_base_m, Some(2650.0));
        assert_eq!(thermals.top_m, 2650.0);
        assert_eq!(thermals.depth_m, 1650.0);
    }

    #[test]
    fn without_heat_from_the_ground_there_are_no_thermals() {
        // A clear night: the air warms the ground.
        let mut hour = afternoon();
        hour.heat_to_air_w_m2 = Some(-40.0);
        let thermals = convection(&hour, GROUND_M).unwrap();
        assert_eq!(thermals.depth_m, 0.0);
        assert_eq!(thermals.updraft_ms, 0.0);
        assert_eq!(thermals.climb_ms, 0.0);
        assert_eq!(thermals.top_m, GROUND_M);
        assert_eq!(thermals.cloud_base_m, None);
        // The wind at the ground is still the model's.
        assert_eq!(thermals.wind_ms, Some(2.0));

        // And a morning with the sun on a ground that lies under an
        // inversion: warmer at 80 m than air risen to it.
        let mut hour = afternoon();
        hour.temperature_c = Some(12.0);
        hour.winds_aloft[0].temperature_c = Some(13.0);
        let thermals = convection(&hour, GROUND_M).unwrap();
        assert_eq!(thermals.depth_m, 0.0);
        assert_eq!(thermals.climb_ms, 0.0);
    }

    #[test]
    fn weak_heat_gives_thermals_no_glider_climbs_in() {
        // A November noon under cloud, as the recorded forecasts have it.
        let mut hour = afternoon();
        hour.heat_to_air_w_m2 = Some(13.9);
        let thermals = convection(&hour, GROUND_M).unwrap();
        assert!(thermals.updraft_ms > 0.5 && thermals.updraft_ms < GLIDER_SINK_MS);
        assert_eq!(thermals.climb_ms, 0.0);
    }

    #[test]
    fn more_heat_and_a_deeper_layer_make_stronger_thermals() {
        let base = convection(&afternoon(), GROUND_M).unwrap();

        let mut hotter = afternoon();
        hotter.heat_to_air_w_m2 = Some(400.0);
        let hotter = convection(&hotter, GROUND_M).unwrap();
        assert_eq!(hotter.top_m, base.top_m);
        assert!(hotter.updraft_ms > base.updraft_ms);

        // Colder air at 3100 m: air from the ground stays warmer for
        // longer and stops higher.
        let mut deeper = afternoon();
        deeper.winds_aloft[4].temperature_c = Some(3.0);
        let deeper = convection(&deeper, GROUND_M).unwrap();
        assert!(deeper.top_m > base.top_m + 300.0);
        assert!(deeper.updraft_ms > base.updraft_ms);
    }

    #[test]
    fn air_still_warmer_at_the_highest_level_ends_there() {
        let mut hour = afternoon();
        hour.dew_point_c = None;
        hour.winds_aloft[4].temperature_c = Some(0.0);
        hour.winds_aloft[5].temperature_c = Some(-12.0);
        let thermals = convection(&hour, GROUND_M).unwrap();
        assert_eq!(thermals.top_m, 4300.0);
        assert_eq!(thermals.wind_ms, Some(12.0));
    }

    #[test]
    fn an_hour_that_lacks_what_the_figures_are_made_from_has_none() {
        let mut hour = afternoon();
        hour.heat_to_air_w_m2 = None;
        assert_eq!(convection(&hour, GROUND_M), None);

        let mut hour = afternoon();
        hour.temperature_c = None;
        assert_eq!(convection(&hour, GROUND_M), None);

        // No level above the ground: a surface under the model's ground
        // does not count, and neither does a level without a temperature.
        let mut hour = afternoon();
        hour.winds_aloft = vec![
            surface(950.0, 560.0, 26.0, 1.0),
            WindAloft::above_ground(80.0),
        ];
        assert_eq!(convection(&hour, GROUND_M), None);
    }

    #[test]
    fn levels_are_read_by_their_height_whatever_their_order() {
        let mut shuffled = afternoon();
        shuffled.winds_aloft.reverse();
        assert_eq!(
            convection(&shuffled, GROUND_M),
            convection(&afternoon(), GROUND_M)
        );
    }
}
