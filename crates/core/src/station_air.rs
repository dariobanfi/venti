//! What the wind stations already say about the air.
//!
//! A station sends more than wind: its temperature and its humidity, and
//! the server passes on how high it stands. Two readings of the air are
//! worked out from those here, each by a fixed rule with its limits named,
//! from the stations a shell already holds for the wind map:
//!
//! - **A cloud base over a station.** The height at which air lifted from
//!   the station would condense, by the rule the thermal estimate uses
//!   (`venti_flight::convection`): 125 m for every degree between the
//!   temperature and the dew point.
//! - **The temperature step to a station higher up or lower down.** How
//!   much the temperature changes for every hundred metres between the two,
//!   with a word for what that says of the air between them.
//!
//! Neither says whether one can fly, and neither is a measurement of the
//! free air: a station measures 2 m above its own ground. The limits are
//! ours and to verify against days that were flown; the decision log in
//! `docs/architecture.md` has them.

use chrono::DateTime;
use venti_domain::GeoPoint;
use venti_flight::convection::{condensation_above_m, dew_point_c};
use venti_flight::sun;

use crate::WindStation;

/// A cloud base is given to the nearest this many metres. The rule it is
/// made by is no finer.
const CLOUD_BASE_STEP_M: f64 = 50.0;

/// The other station of a temperature step stands at least this much
/// higher or lower, metres. Over less height the sensors' own error and
/// the ground under each station outweigh the step. Ours; to verify.
const STEP_HEIGHT_AT_LEAST_M: f64 = 500.0;

/// And no farther away than this over the ground, metres: beyond it the
/// two stand in different air. Ours; to verify.
const STEP_WITHIN_M: f64 = 15_000.0;

/// The two readings of a step were taken no more than this many seconds
/// apart. Ours; stations report every ten minutes.
const STEP_READINGS_APART_S: i64 = 20 * 60;

/// Air that cools by less than this for every hundred metres up, °C, is
/// "hardly cooler above". The standard atmosphere cools by 0.65 and air
/// rising in cloud by about 0.5 to 0.6 at these heights: less than either
/// is a layer that holds rising air back. Ours; to verify.
const HARDLY_COOLER_BELOW_C_PER_100M: f64 = 0.4;

/// Air that cools by this much or more for every hundred metres up, °C, is
/// "much cooler above". Rising dry air cools by 0.98: a layer near that
/// has been stirred through, or is being. Ours; to verify.
const MUCH_COOLER_FROM_C_PER_100M: f64 = 0.8;

/// What a temperature step says of the air between two stations. A
/// description of the layer, by fixed limits; not a word on whether one can
/// fly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum TemperatureStepKind {
    /// The higher station is warmer: an inversion lies between the two.
    WarmerAbove,
    /// As warm above, or cooler by less than 0.4 °C for every 100 m.
    HardlyCoolerAbove,
    /// Cooler by 0.4 °C to less than 0.8 °C for every 100 m, as air at
    /// rest is: the standard atmosphere has 0.65.
    CoolerAbove,
    /// Cooler by 0.8 °C or more for every 100 m: air that turns over.
    MuchCoolerAbove,
}

/// Where air lifted from a station would condense.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct StationCloudBase {
    /// The height, metres above mean sea level, to the nearest fifty.
    pub base_m: f64,
    /// How far above the station that is, metres, to the nearest fifty.
    pub above_station_m: f64,
    /// The dew point at the station it was worked out from, °C, to a
    /// tenth.
    pub dew_point_c: f64,
    /// What the figure is called on a station's page.
    pub words: String,
    /// What it is and is not, to show with it.
    pub note: String,
}

/// The temperature step between a station and one higher up or lower
/// down.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TemperatureStep {
    /// The other station's identifier.
    pub other_id: String,
    /// Its name.
    pub other_name: String,
    /// How far away it stands over the ground, metres.
    pub distance_m: f64,
    /// How much higher it stands, metres; negative when it is the lower
    /// of the two.
    pub higher_by_m: f64,
    /// Its temperature, °C.
    pub other_temperature_c: f64,
    /// How the temperature changes for every hundred metres up, °C, to a
    /// tenth: negative where it is cooler above, as it mostly is.
    pub per_100m_c: f64,
    /// What that says of the air between the two.
    pub kind: TemperatureStepKind,
    /// The same in words: "Warmer above: an inversion".
    pub words: String,
    /// The limits the word stands for, in a sentence.
    pub rule: String,
    /// What the reading is and is not, to show with it.
    pub note: String,
}

/// What a station's reading says about the air, beyond the wind.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct StationAir {
    /// Where air lifted from the station would condense. Absent without
    /// a temperature, a humidity or the station's height, for a reading
    /// that is old, and while the sun is down at the station.
    pub cloud_base: Option<StationCloudBase>,
    /// The temperature step to the nearest station that stands at least
    /// 500 m higher or lower within 15 km. Absent without such a station,
    /// and unless both have a temperature that is not old and the two were
    /// read within 20 minutes of each other.
    pub temperature_step: Option<TemperatureStep>,
}

/// What a station's newest reading says about the air. `stations` are the
/// stations of the wind map, among which the other station of the
/// temperature step is looked for; `station` may be one of them.
///
/// A reading is old by the rule of the wind map ([`WindStation::is_old`]),
/// which was decided against the server's clock when the station was read:
/// nothing here reads a clock.
#[uniffi::export]
pub fn station_air(station: WindStation, stations: Vec<WindStation>) -> StationAir {
    StationAir {
        cloud_base: cloud_base(&station),
        temperature_step: temperature_step(&station, &stations),
    }
}

fn cloud_base(station: &WindStation) -> Option<StationCloudBase> {
    let reading = station.latest.as_ref().filter(|_| !station.is_old)?;
    let elevation_m = station.elevation_m?;
    let temperature_c = reading.temperature_c?;
    let dew_point_c = dew_point_c(temperature_c, reading.humidity_pct?)?;
    // Nothing rises from the ground at night, so there is no base to
    // estimate for it.
    let position = GeoPoint::new(station.position.lat, station.position.lon).ok()?;
    if !sun_is_up(reading.time_unix_s, position) {
        return None;
    }
    let nearest = |metres: f64| (metres / CLOUD_BASE_STEP_M).round() * CLOUD_BASE_STEP_M;
    let above_m = condensation_above_m(temperature_c, dew_point_c);
    Some(StationCloudBase {
        base_m: nearest(elevation_m + above_m),
        above_station_m: nearest(above_m),
        dew_point_c: (dew_point_c * 10.0).round() / 10.0,
        words: "Estimated cloud base".to_owned(),
        note: "Worked out from this station's temperature and humidity: the height at which \
               air lifted from the station would condense, 125 m for every degree between \
               temperature and dew point. An estimate for air that rises from the station's \
               height, not a measurement, and no statement that there is cloud."
            .to_owned(),
    })
}

/// Whether the sun is above a level horizon at a place at a moment.
fn sun_is_up(time_unix_s: i64, position: GeoPoint) -> bool {
    let Some(time) = DateTime::from_timestamp(time_unix_s, 0) else {
        return false;
    };
    let light = sun::daylight_at(time, position);
    matches!(
        (light.sunrise, light.sunset),
        (Some(rise), Some(set)) if (rise..=set).contains(&time)
    )
}

/// A station's height, its current temperature and when that was read.
fn measured(station: &WindStation) -> Option<(f64, f64, i64)> {
    let reading = station.latest.as_ref().filter(|_| !station.is_old)?;
    Some((
        station.elevation_m?,
        reading.temperature_c?,
        reading.time_unix_s,
    ))
}

fn temperature_step(station: &WindStation, stations: &[WindStation]) -> Option<TemperatureStep> {
    let (here_m, here_c, here_s) = measured(station)?;
    let here = GeoPoint::new(station.position.lat, station.position.lon).ok()?;
    let (other, distance_m, there_m, there_c) = stations
        .iter()
        .filter(|other| other.id != station.id)
        .filter_map(|other| {
            let (there_m, there_c, there_s) = measured(other)?;
            let there = GeoPoint::new(other.position.lat, other.position.lon).ok()?;
            let distance_m = here.distance_m(there);
            let fits = (there_m - here_m).abs() >= STEP_HEIGHT_AT_LEAST_M
                && distance_m <= STEP_WITHIN_M
                && (there_s - here_s).abs() <= STEP_READINGS_APART_S;
            fits.then_some((other, distance_m, there_m, there_c))
        })
        // The nearest; of two equally near, always the same one.
        .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.id.cmp(&b.0.id)))?;

    // From the lower station up to the higher, whichever of them was asked
    // about. The word is chosen from the figure as it is shown, to a tenth,
    // so that the two never disagree.
    let tenths = ((there_c - here_c) / (there_m - here_m) * 1000.0).round() as i64;
    let limit = |per_100m_c: f64| (per_100m_c * 10.0).round() as i64;
    let kind = match -tenths {
        cooler if cooler < 0 => TemperatureStepKind::WarmerAbove,
        cooler if cooler < limit(HARDLY_COOLER_BELOW_C_PER_100M) => {
            TemperatureStepKind::HardlyCoolerAbove
        }
        cooler if cooler < limit(MUCH_COOLER_FROM_C_PER_100M) => TemperatureStepKind::CoolerAbove,
        _ => TemperatureStepKind::MuchCoolerAbove,
    };
    Some(TemperatureStep {
        other_id: other.id.clone(),
        other_name: other.name.clone(),
        distance_m,
        higher_by_m: there_m - here_m,
        other_temperature_c: there_c,
        per_100m_c: tenths as f64 / 10.0,
        kind,
        words: step_in_words(kind).to_owned(),
        rule: step_rule(kind),
        note: "Two stations on the ground, not a sounding: each measures the air 2 m above \
               its own ground, which the sun warms and the night cools more than the free \
               air between them. It says nothing about whether one can fly."
            .to_owned(),
    })
}

fn step_in_words(kind: TemperatureStepKind) -> &'static str {
    match kind {
        TemperatureStepKind::WarmerAbove => "Warmer above: an inversion",
        TemperatureStepKind::HardlyCoolerAbove => "Hardly cooler above",
        TemperatureStepKind::CoolerAbove => "Cooler above, as in air at rest",
        TemperatureStepKind::MuchCoolerAbove => "Much cooler above: air that turns over",
    }
}

/// The limits a word stands for, written from the constants so that the
/// sentence cannot say another figure than the rule uses.
fn step_rule(kind: TemperatureStepKind) -> String {
    let (hardly, much) = (HARDLY_COOLER_BELOW_C_PER_100M, MUCH_COOLER_FROM_C_PER_100M);
    match kind {
        TemperatureStepKind::WarmerAbove => {
            "Said where the higher station is warmer than the lower.".to_owned()
        }
        TemperatureStepKind::HardlyCoolerAbove => format!(
            "Said where the higher station is as warm as the lower, or cooler by less than \
             {hardly:.1} °C for every 100 m."
        ),
        TemperatureStepKind::CoolerAbove => format!(
            "Said where it gets cooler by {hardly:.1} °C to less than {much:.1} °C for every \
             100 m up. The standard atmosphere has 0.65 °C."
        ),
        TemperatureStepKind::MuchCoolerAbove => format!(
            "Said where it gets cooler by {much:.1} °C or more for every 100 m up. Rising dry \
             air cools by about 1 °C."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Coordinate, WindMark, WindReading, WindSymbol};

    /// 18 November 2026, 12:00 UTC: early afternoon in the Alps.
    const NOON: i64 = 1_795_003_200;
    /// The same day at 19:05 UTC, long after sunset there.
    const EVENING: i64 = 1_795_028_700;

    /// A made-up station in the Inn valley's longitude, `north_km` north of
    /// a fixed place, with what it measured.
    fn station(
        name: &str,
        north_km: f64,
        elevation_m: Option<f64>,
        temperature_c: Option<f64>,
        humidity_pct: Option<f64>,
    ) -> WindStation {
        WindStation {
            id: format!("made-up:{name}"),
            name: name.to_owned(),
            source: "made-up".to_owned(),
            position: Coordinate {
                // A kilometre is 0.009 degrees of latitude.
                lat: 47.3 + north_km * 0.009,
                lon: 11.0,
            },
            elevation_m,
            latest: Some(WindReading {
                time_unix_s: NOON,
                speed_ms: Some(3.0),
                gust_ms: None,
                direction_deg: Some(270.0),
                temperature_c,
                humidity_pct,
            }),
            age_min: Some(10),
            is_old: false,
            mark: WindMark {
                symbol: WindSymbol::Calm,
                heading_deg: 0.0,
                label: String::new(),
            },
            network: None,
            line: String::new(),
        }
    }

    fn read_at(mut station: WindStation, time_unix_s: i64) -> WindStation {
        if let Some(reading) = station.latest.as_mut() {
            reading.time_unix_s = time_unix_s;
        }
        station
    }

    fn old(mut station: WindStation) -> WindStation {
        station.is_old = true;
        station
    }

    #[test]
    fn a_cloud_base_is_125_metres_above_the_station_for_every_degree_of_spread() {
        // 18 °C at 45 %: a dew point of 5.9 °C, twelve degrees below.
        let valley = station("Valley", 0.0, Some(578.0), Some(18.0), Some(45.0));
        let base = station_air(valley, Vec::new()).cloud_base.unwrap();

        // 1515 m above the station's 578 m is 2093 m: to the nearest fifty.
        assert_eq!(base.base_m, 2100.0);
        assert_eq!(base.above_station_m, 1500.0);
        assert_eq!(base.dew_point_c, 5.9);
        assert_eq!(base.words, "Estimated cloud base");
        // What it is not is said with it.
        assert!(base.note.contains("not a measurement"), "{}", base.note);
        assert!(base.note.contains("no statement that there is cloud"));

        // Saturated air condenses where it is: the station's own height.
        let in_cloud = station("Summit", 0.0, Some(2247.0), Some(1.5), Some(100.0));
        let base = station_air(in_cloud, Vec::new()).cloud_base.unwrap();
        assert_eq!((base.base_m, base.above_station_m), (2250.0, 0.0));
    }

    #[test]
    fn a_cloud_base_is_left_out_where_it_cannot_be_said() {
        let said = |station: WindStation| station_air(station, Vec::new()).cloud_base.is_some();
        let whole = || station("Valley", 0.0, Some(578.0), Some(18.0), Some(45.0));
        assert!(said(whole()));

        // Without a temperature, a humidity or the station's height.
        assert!(!said(station("x", 0.0, Some(578.0), None, Some(45.0))));
        assert!(!said(station("x", 0.0, Some(578.0), Some(18.0), None)));
        assert!(!said(station("x", 0.0, None, Some(18.0), Some(45.0))));
        // A humidity that is none.
        assert!(!said(station("x", 0.0, Some(578.0), Some(18.0), Some(0.0))));
        // A reading the wind map shows as old.
        assert!(!said(old(whole())));
        // After sunset nothing rises from the ground.
        assert!(!said(read_at(whole(), EVENING)));
        // A station that never reported.
        let mut silent = whole();
        silent.latest = None;
        assert!(!said(silent));
    }

    #[test]
    fn the_step_is_taken_to_the_nearest_station_far_enough_above_that_can_be_compared() {
        let valley = station("Valley", 0.0, Some(600.0), Some(12.0), None);
        let stations = vec![
            valley.clone(),
            // Nearer, and not 500 m higher.
            station("Slope", 3.0, Some(900.0), Some(10.0), None),
            // High enough and nearer than the summit, but each unfit: an old
            // reading, one read 40 minutes earlier, one without a
            // temperature, one whose height is not known.
            old(station("Old", 5.0, Some(1500.0), Some(6.0), None)),
            read_at(
                station("Early", 6.0, Some(1300.0), Some(7.0), None),
                NOON - 40 * 60,
            ),
            station("Wind only", 7.0, Some(1400.0), None, None),
            station("No height", 7.5, None, Some(5.0), None),
            station("Summit", 8.0, Some(1800.0), Some(4.8), None),
            // Higher still, and beyond 15 km.
            station("Far", 20.0, Some(2900.0), Some(-2.0), None),
        ];

        let step = station_air(valley, stations.clone())
            .temperature_step
            .unwrap();
        assert_eq!(step.other_id, "made-up:Summit");
        assert_eq!(step.other_name, "Summit");
        assert!((step.distance_m - 8000.0).abs() < 50.0, "{step:?}");
        assert_eq!(step.higher_by_m, 1200.0);
        assert_eq!(step.other_temperature_c, 4.8);
        // 7.2 °C over 1200 m.
        assert_eq!(step.per_100m_c, -0.6);
        assert_eq!(step.kind, TemperatureStepKind::CoolerAbove);
        assert_eq!(step.words, "Cooler above, as in air at rest");
        assert!(
            step.rule.contains("0.4 °C to less than 0.8 °C"),
            "{}",
            step.rule
        );
        assert!(step.note.contains("not a sounding"), "{}", step.note);
        assert!(step.note.contains("nothing about whether one can fly"));

        // Asked from the summit, the same step is read down to the valley:
        // the figure is always from the lower station up.
        let (valley, summit) = (stations[0].clone(), stations[6].clone());
        let down = station_air(summit.clone(), vec![valley, summit.clone()])
            .temperature_step
            .unwrap();
        assert_eq!(down.other_name, "Valley");
        assert_eq!(down.higher_by_m, -1200.0);
        assert_eq!(down.other_temperature_c, 12.0);
        assert_eq!((down.per_100m_c, down.kind), (step.per_100m_c, step.kind));

        // Among all of them the summit's own nearest is the slope, 900 m
        // below it and 5 km away: each station has its own other one.
        let nearest = station_air(summit, stations).temperature_step.unwrap();
        assert_eq!(nearest.other_name, "Slope");
        assert_eq!(nearest.higher_by_m, -900.0);
    }

    #[test]
    fn a_step_has_one_of_four_words_by_fixed_limits() {
        // A valley station at 12 °C and one 1200 m above it.
        let kind_at = |summit_c: f64| {
            let valley = station("Valley", 0.0, Some(600.0), Some(12.0), None);
            let summit = station("Summit", 8.0, Some(1800.0), Some(summit_c), None);
            let step = station_air(valley, vec![summit]).temperature_step.unwrap();
            (step.per_100m_c, step.kind, step.words)
        };
        use TemperatureStepKind::{CoolerAbove, HardlyCoolerAbove, MuchCoolerAbove, WarmerAbove};

        let (per_100m_c, kind, words) = kind_at(14.4);
        assert_eq!((per_100m_c, kind), (0.2, WarmerAbove));
        assert_eq!(words, "Warmer above: an inversion");
        // As warm above is no inversion yet.
        assert_eq!(kind_at(12.0).1, HardlyCoolerAbove);
        let (per_100m_c, kind, words) = kind_at(10.8);
        assert_eq!((per_100m_c, kind), (-0.1, HardlyCoolerAbove));
        assert_eq!(words, "Hardly cooler above");
        // The limits belong to the stronger word.
        assert_eq!(
            kind_at(7.2),
            (
                -0.4,
                CoolerAbove,
                "Cooler above, as in air at rest".to_owned()
            )
        );
        assert_eq!(kind_at(3.6).1, CoolerAbove);
        let (per_100m_c, kind, words) = kind_at(2.4);
        assert_eq!((per_100m_c, kind), (-0.8, MuchCoolerAbove));
        assert_eq!(words, "Much cooler above: air that turns over");
        assert_eq!(kind_at(0.0), (-1.0, MuchCoolerAbove, words));
        // The word follows the figure as it is shown: 0.37 °C is shown as
        // 0.4 and named as 0.4.
        assert_eq!(kind_at(12.0 - 0.37 * 12.0).1, CoolerAbove);
    }

    #[test]
    fn a_step_is_left_out_without_a_station_to_compare_or_a_current_temperature() {
        let valley = || station("Valley", 0.0, Some(600.0), Some(12.0), None);
        let summit = || station("Summit", 8.0, Some(1800.0), Some(4.8), None);
        let said = |station: WindStation, stations: Vec<WindStation>| {
            station_air(station, stations).temperature_step.is_some()
        };
        assert!(said(valley(), vec![summit()]));

        // Alone, or with only itself in the list.
        assert!(!said(valley(), Vec::new()));
        assert!(!said(valley(), vec![valley()]));
        // The station's own reading is old, or lacks what is compared.
        assert!(!said(old(valley()), vec![summit()]));
        assert!(!said(
            station("Valley", 0.0, Some(600.0), None, None),
            vec![summit()]
        ));
        assert!(!said(
            station("Valley", 0.0, None, Some(12.0), None),
            vec![summit()]
        ));
        // At night the step is still said: an inversion is a thing of the
        // night as much as of the day.
        assert!(said(
            read_at(valley(), EVENING),
            vec![read_at(summit(), EVENING)]
        ));
    }
}
