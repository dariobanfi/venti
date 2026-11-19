//! Putting a published limit and a pilot on one height scale.
//!
//! A limit is published against one of three things: mean sea level, the
//! ground below, or the standard atmosphere (flight levels). The pilot's
//! altitude comes from satellites. Comparing the two needs the ground
//! elevation for the second kind and the air pressure for the third, and a
//! phone does not always have them.
//!
//! The rule of this module: **what is not known is taken toward the
//! pilot.** A limit that cannot be placed exactly is given as a range, the
//! floor is taken at the bottom of its range and the ceiling at the top of
//! its own, and the answer says that it is uncertain and why. It never says
//! "clear" on a guess.

use venti_domain::{Airspace, VerticalLimit, VerticalReference};

/// Sea-level pressure of the standard atmosphere, hectopascals. Flight
/// levels are pressure altitudes at this setting.
pub const STANDARD_PRESSURE_HPA: f64 = 1013.25;

/// The lowest the ground is taken to be when its elevation is unknown,
/// metres above sea level. The lowest land on Earth is about 430 m below it.
pub const LOWEST_GROUND_M: f64 = -500.0;

/// The highest the ground is taken to be, metres above sea level.
pub const HIGHEST_GROUND_M: f64 = 9000.0;

/// Lowest sea-level pressure assumed when a flight level has to be placed
/// without a pressure sensor, hectopascals. Pressure this low is a deep
/// winter storm; on a flyable day it is higher.
pub const LOWEST_QNH_HPA: f64 = 980.0;

/// Highest sea-level pressure assumed in the same case, hectopascals.
pub const HIGHEST_QNH_HPA: f64 = 1040.0;

/// How far the true height of a pressure level is taken to differ from its
/// height in the standard atmosphere because the air is warmer or colder
/// than standard, as a fraction of that height: 7 %, which is about 20 °C.
pub const TEMPERATURE_SPREAD: f64 = 0.07;

/// The altitude at which the standard atmosphere has the pressure `hpa`,
/// metres. The troposphere part of the International Standard Atmosphere,
/// the same formula the flight computer uses.
pub fn standard_altitude_m(hpa: f64) -> f64 {
    44_330.77 * (1.0 - (hpa / STANDARD_PRESSURE_HPA).powf(0.190_263))
}

/// The pressure the standard atmosphere has at `altitude_m`, hectopascals.
fn standard_pressure_hpa(altitude_m: f64) -> f64 {
    STANDARD_PRESSURE_HPA
        * (1.0 - altitude_m / 44_330.77)
            .max(0.0)
            .powf(1.0 / 0.190_263)
}

/// An altitude worked out from air pressure, with the sea-level pressure it
/// was worked out for.
///
/// With the standard 1013.25 hPa it is the pressure altitude that flight
/// levels are defined in, which is what a flight computer or a flight
/// recorder gives. Prefer that, or [`PressureAltitude::from_pressure`].
///
/// An altitude worked out for another sea-level pressure (a QNH) is
/// accepted too and converted. It is taken to come from the formula that
/// phones and varios use, `44330.77 × (1 − (pressure / reference)^0.190263)`.
/// An aircraft altimeter shifts its scale instead of stretching it and reads
/// a few metres differently at height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressureAltitude {
    /// The altitude in metres.
    pub altitude_m: f64,
    /// The sea-level pressure it refers to, hectopascals.
    pub reference_hpa: f64,
}

impl PressureAltitude {
    /// A pressure altitude on the standard atmosphere, 1013.25 hPa.
    pub fn standard(altitude_m: f64) -> Self {
        Self {
            altitude_m,
            reference_hpa: STANDARD_PRESSURE_HPA,
        }
    }

    /// The pressure altitude a measured pressure stands for.
    pub fn from_pressure(hpa: f64) -> Self {
        Self::standard(standard_altitude_m(hpa))
    }

    /// The altitude on the standard atmosphere, metres: the number that
    /// flight levels are counted in.
    pub fn standard_m(&self) -> f64 {
        if self.reference_hpa == STANDARD_PRESSURE_HPA {
            return self.altitude_m;
        }
        // Back to the pressure that was measured, then onto the standard scale.
        let scale = self.reference_hpa / STANDARD_PRESSURE_HPA;
        standard_altitude_m(standard_pressure_hpa(self.altitude_m) * scale)
    }
}

/// What is known about how high the pilot is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Altitudes {
    /// Altitude above mean sea level in metres: from the satellites, or the
    /// cockpit's best altitude (pressure altitude kept true by the
    /// satellites). Limits above mean sea level are compared with this.
    pub altitude_m: f64,
    /// The pressure altitude, if a pressure sensor is there. Flight levels
    /// are compared with this. Without it they are placed by the standard
    /// atmosphere with a wide margin.
    pub pressure: Option<PressureAltitude>,
    /// Elevation of the ground below the pilot in metres above mean sea
    /// level, if known. Limits above ground are placed with this. Without
    /// it they are taken to reach the pilot.
    pub ground_elevation_m: Option<f64>,
}

impl Altitudes {
    /// Only the satellite altitude is known.
    pub fn gps(altitude_m: f64) -> Self {
        Self {
            altitude_m,
            pressure: None,
            ground_elevation_m: None,
        }
    }
}

/// Why a limit could not be placed exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unknown {
    /// The limit is measured from the ground and the ground elevation below
    /// the pilot is not known.
    GroundElevation,
    /// The limit is a flight level and no pressure altitude is available.
    Pressure,
}

/// A published limit on the pilot's own height scale: metres above mean sea
/// level as the satellites measure it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedLimit {
    /// The lowest the limit can be.
    pub low_m: f64,
    /// The highest the limit can be. Equal to `low_m` when the limit is
    /// known exactly; infinite for "unlimited".
    pub high_m: f64,
    /// Why the two differ, when they do.
    pub unknown: Option<Unknown>,
}

impl ResolvedLimit {
    fn exactly(metres: f64) -> Self {
        Self {
            low_m: metres,
            high_m: metres,
            unknown: None,
        }
    }

    /// Whether the limit is known exactly.
    pub fn is_exact(&self) -> bool {
        self.low_m == self.high_m
    }

    /// The higher of two limits, for a floor published as "X but at least Y".
    fn higher(self, other: ResolvedLimit) -> ResolvedLimit {
        ResolvedLimit {
            low_m: self.low_m.max(other.low_m),
            high_m: self.high_m.max(other.high_m),
            unknown: self.unknown.or(other.unknown),
        }
    }
}

/// Places one limit on the pilot's height scale.
///
/// - **Above mean sea level:** as published.
/// - **Ground:** the ground elevation if known, but never above the pilot,
///   who cannot be under the ground.
/// - **Above ground:** exact with a ground elevation. Without one the ground
///   may lie anywhere from [`LOWEST_GROUND_M`] up to the pilot's own
///   altitude, and so may the limit above it.
/// - **Flight level:** with a pressure altitude, the pilot's altitude plus
///   the difference in pressure altitude, which is how the rule is written.
///   Without one, the standard atmosphere, widened by the weather it may be
///   wrong by: see [`flight_level_range_m`].
/// - **Unlimited:** infinitely high.
pub fn resolve(limit: &VerticalLimit, altitudes: &Altitudes) -> ResolvedLimit {
    let altitude = altitudes.altitude_m;
    let Some(metres) = limit.metres() else {
        return ResolvedLimit::exactly(f64::INFINITY);
    };
    match limit.reference() {
        VerticalReference::Unlimited => ResolvedLimit::exactly(f64::INFINITY),
        VerticalReference::MeanSeaLevel => ResolvedLimit::exactly(metres),
        VerticalReference::Ground => match altitudes.ground_elevation_m {
            Some(ground) => ResolvedLimit::exactly(ground.min(altitude)),
            None => ResolvedLimit {
                low_m: LOWEST_GROUND_M.min(altitude),
                high_m: altitude,
                unknown: Some(Unknown::GroundElevation),
            },
        },
        VerticalReference::AboveGround => match altitudes.ground_elevation_m {
            Some(ground) => ResolvedLimit::exactly(ground + metres),
            None => ResolvedLimit {
                low_m: (LOWEST_GROUND_M + metres).min(altitude + metres),
                high_m: altitude + metres,
                unknown: Some(Unknown::GroundElevation),
            },
        },
        VerticalReference::Standard => match altitudes.pressure {
            Some(pressure) => ResolvedLimit::exactly(altitude + metres - pressure.standard_m()),
            None => {
                let (low_m, high_m) = flight_level_range_m(metres);
                ResolvedLimit {
                    low_m,
                    high_m,
                    unknown: Some(Unknown::Pressure),
                }
            }
        },
    }
}

/// Where a pressure altitude of `standard_m` metres may truly lie above mean
/// sea level when nothing is known about the day's weather.
///
/// A flight level is a surface of constant pressure. It lies lower than its
/// nominal height when the sea-level pressure is below 1013.25 hPa and when
/// the air is colder than standard, and higher in the opposite case. The
/// range covers sea-level pressures from [`LOWEST_QNH_HPA`] to
/// [`HIGHEST_QNH_HPA`] and [`TEMPERATURE_SPREAD`] either way. For FL 100,
/// nominally 3048 m, that is 2573 m to 3497 m.
///
/// On a day outside those bounds, or once a pressure sensor is connected,
/// the range is wrong or unnecessary: give the query a pressure altitude and
/// the comparison is exact.
pub fn flight_level_range_m(standard_m: f64) -> (f64, f64) {
    let low = (standard_m - standard_altitude_m(LOWEST_QNH_HPA)) * (1.0 - TEMPERATURE_SPREAD);
    let high = (standard_m - standard_altitude_m(HIGHEST_QNH_HPA)) * (1.0 + TEMPERATURE_SPREAD);
    (low.min(standard_m), high.max(standard_m))
}

/// The range of altitudes above mean sea level a limit can have anywhere,
/// knowing nothing about the pilot: for filtering airspace by height.
pub(crate) fn widest_range_m(limit: &VerticalLimit) -> (f64, f64) {
    let Some(metres) = limit.metres() else {
        return (f64::INFINITY, f64::INFINITY);
    };
    match limit.reference() {
        VerticalReference::Unlimited => (f64::INFINITY, f64::INFINITY),
        VerticalReference::MeanSeaLevel => (metres, metres),
        VerticalReference::Ground | VerticalReference::AboveGround => {
            (LOWEST_GROUND_M + metres, HIGHEST_GROUND_M + metres)
        }
        VerticalReference::Standard => flight_level_range_m(metres),
    }
}

/// Where the pilot is in relation to an airspace's floor and ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerticalRelation {
    /// Certainly below the floor.
    Below,
    /// Between floor and ceiling, or possibly so: see [`VerticalStatus::uncertain`].
    Inside,
    /// Certainly above the ceiling.
    Above,
}

/// The pilot's height in relation to one airspace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalStatus {
    /// Below, inside or above.
    pub relation: VerticalRelation,
    /// Metres of air between the pilot and the airspace when below or above
    /// it: the least it can be. Zero when inside.
    pub separation_m: f64,
    /// How far the pilot is above the lowest the floor can be, metres.
    /// Negative when below the airspace. When inside, this is how far down
    /// the way out is.
    pub above_floor_m: f64,
    /// How far the pilot is below the highest the ceiling can be, metres.
    /// Negative when above the airspace, infinite under an unlimited ceiling.
    pub below_ceiling_m: f64,
    /// The floor on the pilot's height scale.
    pub floor: ResolvedLimit,
    /// The ceiling on the pilot's height scale.
    pub ceiling: ResolvedLimit,
    /// The pilot is reported inside only because something is unknown: with
    /// the ground elevation or the pressure the answer might be "below" or
    /// "above". `floor.unknown` and `ceiling.unknown` say what is missing.
    pub uncertain: bool,
}

/// Works out where the pilot is in relation to an airspace's floor and ceiling.
///
/// The floor is taken as low as it can be and the ceiling as high as it can
/// be, so "below" and "above" are only said when they are certain.
pub fn vertical_status(airspace: &Airspace, altitudes: &Altitudes) -> VerticalStatus {
    let altitude = altitudes.altitude_m;

    let mut floor = resolve(&airspace.lower, altitudes);
    if let Some(at_least) = &airspace.lower_at_least {
        // "5000 ft AMSL but at least 1000 ft AGL": the higher of the two.
        // When the second cannot be placed, its lowest value leaves the
        // published floor in charge.
        floor = floor.higher(resolve(at_least, altitudes));
    }
    let mut ceiling = resolve(&airspace.upper, altitudes);
    if let Some(at_most) = &airspace.upper_at_most {
        // The ceiling may be raised to this: count on it.
        let raised = resolve(at_most, altitudes);
        ceiling = ResolvedLimit {
            low_m: ceiling.low_m,
            high_m: ceiling.high_m.max(raised.high_m),
            unknown: ceiling.unknown.or(raised.unknown),
        };
    }

    let above_floor_m = altitude - floor.low_m;
    let below_ceiling_m = ceiling.high_m - altitude;
    let (relation, separation_m) = if above_floor_m < 0.0 {
        (VerticalRelation::Below, -above_floor_m)
    } else if below_ceiling_m < 0.0 {
        (VerticalRelation::Above, -below_ceiling_m)
    } else {
        (VerticalRelation::Inside, 0.0)
    };
    let uncertain = relation == VerticalRelation::Inside
        && (altitude < floor.high_m || altitude > ceiling.low_m);

    VerticalStatus {
        relation,
        separation_m,
        above_floor_m,
        below_ceiling_m,
        floor,
        ceiling,
        uncertain,
    }
}

#[cfg(test)]
mod tests {
    use venti_domain::{
        Activity, AirspaceClass, AirspaceType, GeoPoint, HeightUnit, Polygon, Validity,
    };

    use super::*;

    fn feet(reference: VerticalReference, value: f64) -> VerticalLimit {
        VerticalLimit::new(reference, value, HeightUnit::Feet).unwrap()
    }

    fn airspace(lower: VerticalLimit, upper: VerticalLimit) -> Airspace {
        let point = |lat, lon| GeoPoint::new(lat, lon).unwrap();
        let ring = vec![
            point(47.0, 11.0),
            point(47.0, 11.2),
            point(47.1, 11.2),
            point(47.0, 11.0),
        ];
        Airspace {
            id: "test:a".parse().unwrap(),
            name: "TEST".to_owned(),
            class: AirspaceClass::C,
            airspace_type: AirspaceType::Tma,
            country: None,
            lower,
            upper,
            lower_at_least: None,
            upper_at_most: None,
            geometry: Polygon::new(ring, Vec::new()).unwrap(),
            activity: Activity::default(),
            frequencies: Vec::new(),
            remarks: None,
            validity: Validity::default(),
        }
    }

    #[test]
    fn the_standard_atmosphere_matches_its_table() {
        for (hpa, metres) in [(1013.25, 0.0), (898.75, 1000.0), (701.09, 3000.0)] {
            assert!((standard_altitude_m(hpa) - metres).abs() < 2.0);
            assert!((standard_pressure_hpa(metres) - hpa).abs() < 0.2);
        }
    }

    #[test]
    fn an_altimeter_setting_is_converted_to_the_standard_one() {
        // An altitude for 1013.25 hPa already is the pressure altitude.
        assert_eq!(PressureAltitude::standard(2500.0).standard_m(), 2500.0);
        assert!((PressureAltitude::from_pressure(701.09).standard_m() - 3000.0).abs() < 2.0);

        // At sea level, a setting of 990 hPa reads 195 m less than the
        // standard one: the height at which the standard atmosphere has 990 hPa.
        let offset = standard_altitude_m(990.0);
        assert!((offset - 195.4).abs() < 0.5, "{offset}");
        let at_sea_level = PressureAltitude {
            altitude_m: 0.0,
            reference_hpa: 990.0,
        };
        assert!((at_sea_level.standard_m() - offset).abs() < 1e-6);

        // The same measured pressure gives the same pressure altitude
        // whichever setting the altitude was worked out for.
        let measured_hpa = 715.0;
        let altitude_for = |reference_hpa: f64| PressureAltitude {
            altitude_m: 44_330.77 * (1.0 - (measured_hpa / reference_hpa).powf(0.190_263)),
            reference_hpa,
        };
        let expected = standard_altitude_m(measured_hpa);
        for reference in [990.0, 1013.25, 1030.0] {
            let standard = altitude_for(reference).standard_m();
            assert!(
                (standard - expected).abs() < 1e-6,
                "{reference}: {standard}"
            );
        }
        // On a low-pressure day the setting reads lower than the pressure altitude.
        assert!(altitude_for(990.0).altitude_m < expected - 150.0);
    }

    #[test]
    fn limits_above_sea_level_are_taken_as_published() {
        let limit = feet(VerticalReference::MeanSeaLevel, 9500.0);
        let resolved = resolve(&limit, &Altitudes::gps(2000.0));
        assert!(resolved.is_exact() && (resolved.low_m - 2895.6).abs() < 1e-9);
        assert_eq!(resolved.unknown, None);
        let unlimited = resolve(&VerticalLimit::unlimited(), &Altitudes::gps(2000.0));
        assert!(unlimited.low_m.is_infinite() && unlimited.is_exact());
    }

    #[test]
    fn a_floor_at_fl100_on_a_low_pressure_day() {
        // FL 100 is 3048 m of pressure altitude. With 990 hPa at sea level
        // the air at 2850 m is as thin as the standard atmosphere's at
        // 3045 m: a pilot the satellites put at 2850 m is 3 m under the
        // floor, not 198 m.
        let tma = airspace(
            VerticalLimit::flight_level(100).unwrap(),
            VerticalLimit::unlimited(),
        );
        let offset = standard_altitude_m(990.0);

        let just_below = Altitudes {
            altitude_m: 2850.0,
            pressure: Some(PressureAltitude::standard(2850.0 + offset)),
            ground_elevation_m: None,
        };
        let status = vertical_status(&tma, &just_below);
        assert_eq!(status.relation, VerticalRelation::Below);
        assert!((status.separation_m - (3048.0 - 2850.0 - offset)).abs() < 1e-6);
        assert!(status.separation_m < 3.0);
        assert!(!status.uncertain && status.floor.is_exact());

        // Ten metres higher the pilot is in it, 188 m below where the
        // number on the chart would put the floor.
        let inside = Altitudes {
            altitude_m: 2860.0,
            pressure: Some(PressureAltitude::standard(2860.0 + offset)),
            ground_elevation_m: None,
        };
        let status = vertical_status(&tma, &inside);
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(!status.uncertain);
        assert!(
            (status.above_floor_m - 7.4).abs() < 0.5,
            "{}",
            status.above_floor_m
        );

        // The same air pressure, given as an altitude worked out for the
        // day's sea-level pressure instead of the standard one.
        let measured_hpa = standard_pressure_hpa(2860.0 + offset);
        let on_qnh = Altitudes {
            altitude_m: 2860.0,
            pressure: Some(PressureAltitude {
                altitude_m: 44_330.77 * (1.0 - (measured_hpa / 990.0).powf(0.190_263)),
                reference_hpa: 990.0,
            }),
            ground_elevation_m: None,
        };
        let status = vertical_status(&tma, &on_qnh);
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!((status.above_floor_m - 7.4).abs() < 0.5);
    }

    #[test]
    fn without_pressure_a_flight_level_reaches_down_to_the_pilot() {
        let tma = airspace(
            VerticalLimit::flight_level(100).unwrap(),
            VerticalLimit::unlimited(),
        );
        let (low, high) = flight_level_range_m(3048.0);
        assert!((low - 2573.0).abs() < 2.0, "{low}");
        assert!((high - 3497.0).abs() < 2.0, "{high}");

        // 250 m under the nominal floor, with no barometer: possibly in it.
        let status = vertical_status(&tma, &Altitudes::gps(2800.0));
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(status.uncertain);
        assert_eq!(status.floor.unknown, Some(Unknown::Pressure));

        // Far enough below, the answer is certain even without one.
        let status = vertical_status(&tma, &Altitudes::gps(2500.0));
        assert_eq!(status.relation, VerticalRelation::Below);
        assert!((status.separation_m - (low - 2500.0)).abs() < 1e-9);
        assert!(!status.uncertain);

        // Well above the highest the floor can be, the pilot is in it for sure.
        let status = vertical_status(&tma, &Altitudes::gps(3600.0));
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(!status.uncertain);
    }

    #[test]
    fn a_ceiling_at_a_flight_level_is_taken_as_high_as_it_can_be() {
        let below = airspace(
            VerticalLimit::ground(),
            VerticalLimit::flight_level(65).unwrap(),
        );
        // FL 65 is 1981 m. At 2300 m without a barometer the pilot may
        // still be under it on a warm high-pressure day.
        let status = vertical_status(&below, &Altitudes::gps(2300.0));
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(status.uncertain);
        let status = vertical_status(&below, &Altitudes::gps(2500.0));
        assert_eq!(status.relation, VerticalRelation::Above);
        assert!(!status.uncertain);
    }

    #[test]
    fn limits_above_ground_need_the_ground() {
        // A sector from 1000 ft above ground to 4500 ft above sea level.
        let sector = airspace(
            feet(VerticalReference::AboveGround, 1000.0),
            feet(VerticalReference::MeanSeaLevel, 4500.0),
        );
        // Ground at 700 m: the floor is at 1004.8 m.
        let known = |altitude_m| Altitudes {
            altitude_m,
            pressure: None,
            ground_elevation_m: Some(700.0),
        };
        let status = vertical_status(&sector, &known(900.0));
        assert_eq!(status.relation, VerticalRelation::Below);
        assert!((status.separation_m - 104.8).abs() < 1e-9);
        let status = vertical_status(&sector, &known(1100.0));
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(!status.uncertain);
        assert!((status.below_ceiling_m - 271.6).abs() < 1e-9);

        // Ground unknown: at 900 m the pilot may be over low ground and
        // above the floor. Reported inside, and uncertain.
        let status = vertical_status(&sector, &Altitudes::gps(900.0));
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(status.uncertain);
        assert_eq!(status.floor.unknown, Some(Unknown::GroundElevation));
        // Above the ceiling, which is above sea level, the answer is certain.
        let status = vertical_status(&sector, &Altitudes::gps(1500.0));
        assert_eq!(status.relation, VerticalRelation::Above);
    }

    #[test]
    fn a_ceiling_above_ground_without_the_ground_is_never_cleared() {
        let low = airspace(
            VerticalLimit::ground(),
            feet(VerticalReference::AboveGround, 1000.0),
        );
        // However high the pilot is, the ground below may be high too.
        for altitude in [200.0, 2000.0, 4000.0] {
            let status = vertical_status(&low, &Altitudes::gps(altitude));
            assert_eq!(status.relation, VerticalRelation::Inside, "{altitude}");
            assert!(status.uncertain);
        }
        let known = Altitudes {
            altitude_m: 2000.0,
            pressure: None,
            ground_elevation_m: Some(600.0),
        };
        assert_eq!(
            vertical_status(&low, &known).relation,
            VerticalRelation::Above
        );
    }

    #[test]
    fn a_floor_at_the_ground_never_has_the_pilot_under_it() {
        let ctr = airspace(
            VerticalLimit::ground(),
            feet(VerticalReference::MeanSeaLevel, 5000.0),
        );
        // Ground unknown: certain all the same.
        let status = vertical_status(&ctr, &Altitudes::gps(700.0));
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(!status.uncertain);
        // A terrain model that puts the ground above the pilot's satellite
        // altitude must not lift the pilot out of the airspace.
        let standing = Altitudes {
            altitude_m: 575.0,
            pressure: None,
            ground_elevation_m: Some(581.0),
        };
        let status = vertical_status(&ctr, &standing);
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(!status.uncertain);
        // Below sea level too.
        let status = vertical_status(&ctr, &Altitudes::gps(-600.0));
        assert_eq!(status.relation, VerticalRelation::Inside);
    }

    #[test]
    fn a_second_floor_raises_the_floor_only_when_it_can_be_placed() {
        // "11000 ft AMSL but at least 1500 ft above ground."
        let mut tmz = airspace(
            feet(VerticalReference::MeanSeaLevel, 11_000.0),
            VerticalLimit::flight_level(125).unwrap(),
        );
        tmz.lower_at_least = Some(feet(VerticalReference::AboveGround, 1500.0));
        let pressure = Some(PressureAltitude::standard(3400.0));

        // Over a 3000 m ridge the floor is 3457 m, above the published 3353 m.
        let over_ridge = Altitudes {
            altitude_m: 3400.0,
            pressure,
            ground_elevation_m: Some(3000.0),
        };
        let status = vertical_status(&tmz, &over_ridge);
        assert_eq!(status.relation, VerticalRelation::Below);
        assert!((status.separation_m - 57.2).abs() < 1e-9);

        // Over the valley the published floor holds.
        let over_valley = Altitudes {
            altitude_m: 3400.0,
            pressure,
            ground_elevation_m: Some(600.0),
        };
        assert_eq!(
            vertical_status(&tmz, &over_valley).relation,
            VerticalRelation::Inside
        );

        // Ground unknown: the published floor holds, and the answer says
        // that the ground might have raised it.
        let unknown = Altitudes {
            altitude_m: 3400.0,
            pressure,
            ground_elevation_m: None,
        };
        let status = vertical_status(&tmz, &unknown);
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(status.uncertain);
        assert!((status.floor.low_m - 3352.8).abs() < 1e-9);
    }

    #[test]
    fn a_ceiling_that_may_be_raised_counts_as_raised() {
        let mut area = airspace(
            VerticalLimit::ground(),
            feet(VerticalReference::MeanSeaLevel, 5000.0),
        );
        area.upper_at_most = Some(feet(VerticalReference::MeanSeaLevel, 8000.0));
        let status = vertical_status(&area, &Altitudes::gps(2000.0));
        assert_eq!(status.relation, VerticalRelation::Inside);
        assert!(status.uncertain);
        let status = vertical_status(&area, &Altitudes::gps(2500.0));
        assert_eq!(status.relation, VerticalRelation::Above);
    }

    #[test]
    fn the_widest_range_covers_every_place_and_day() {
        let (low, high) = widest_range_m(&feet(VerticalReference::AboveGround, 1000.0));
        assert!(low < 0.0 && high > 9000.0);
        let (low, high) = widest_range_m(&feet(VerticalReference::MeanSeaLevel, 1000.0));
        assert_eq!(low, high);
        assert!(widest_range_m(&VerticalLimit::unlimited()).0.is_infinite());
        let (low, high) = widest_range_m(&VerticalLimit::flight_level(100).unwrap());
        assert!(low < 2600.0 && high > 3450.0);
    }
}
