//! The standard atmosphere, for turning pressure into altitude.

/// Sea-level pressure of the standard atmosphere, hectopascals.
pub const STANDARD_SEA_LEVEL_HPA: f64 = 1013.25;

/// The altitude at which the standard atmosphere has this pressure, metres.
///
/// This is "pressure altitude": what an altimeter set to 1013.25 hPa shows
/// and what flight files record. It differs from true altitude by the
/// weather, typically by tens of metres and on some days by a few hundred.
/// The formula is the troposphere part of the International Standard
/// Atmosphere and holds up to 11 km.
pub fn pressure_altitude_m(pressure_hpa: f64) -> f64 {
    44_330.77 * (1.0 - (pressure_hpa / STANDARD_SEA_LEVEL_HPA).powf(0.190_263))
}

/// The pressure the standard atmosphere has at an altitude, hectopascals:
/// [`pressure_altitude_m`] the other way round.
pub fn pressure_hpa_at(altitude_m: f64) -> f64 {
    STANDARD_SEA_LEVEL_HPA * (1.0 - altitude_m / 44_330.77).powf(1.0 / 0.190_263)
}

/// The QNH: the altimeter setting at which an altimeter that reads
/// `standard_altitude_m` at 1013.25 hPa shows `elevation_m`, the known
/// height of the place it stands at. An altimeter's scale is the standard
/// atmosphere's, so a setting shifts its reading by the pressure altitude
/// of the setting, whatever the height.
pub fn qnh_hpa(standard_altitude_m: f64, elevation_m: f64) -> f64 {
    pressure_hpa_at(standard_altitude_m - elevation_m)
}

/// What an altimeter set to a QNH shows, metres above sea level, from the
/// pressure altitude. It is the altitude airspace limits above sea level
/// are flown by. It is true at the height the QNH was taken at and drifts
/// from the true altitude above it by about 4% of the height difference
/// for every 10 °C the air is warmer or colder than the standard
/// atmosphere: reading low on a warm day.
pub fn altitude_at_qnh_m(standard_altitude_m: f64, qnh_hpa: f64) -> f64 {
    standard_altitude_m - pressure_altitude_m(qnh_hpa)
}

/// The QNHs taken as real, hectopascals: the lowest and highest pressures
/// ever measured at sea level lie inside.
pub const QNH_HPA: std::ops::RangeInclusive<f64> = 870.0..=1085.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_qnh_taken_at_a_known_height_shows_that_height_and_shifts_every_other() {
        // 850 hPa at a take-off 1,500 m up: the standard atmosphere has
        // 845.6 hPa there, so the day's pressure is a little high.
        let on_the_hill = pressure_altitude_m(850.0);
        let qnh = qnh_hpa(on_the_hill, 1_500.0);
        assert!((qnh - 1018.5).abs() < 0.5, "{qnh}");
        assert!((altitude_at_qnh_m(on_the_hill, qnh) - 1_500.0).abs() < 0.01);
        // A thousand metres of pressure altitude higher it shows a
        // thousand metres more.
        assert!((altitude_at_qnh_m(on_the_hill + 1_000.0, qnh) - 2_500.0).abs() < 0.01);
        // At 1013.25 it is the pressure altitude itself.
        assert!((altitude_at_qnh_m(2_000.0, STANDARD_SEA_LEVEL_HPA) - 2_000.0).abs() < 0.01);
        assert!((pressure_hpa_at(pressure_altitude_m(701.09)) - 701.09).abs() < 0.01);
    }

    #[test]
    fn matches_the_standard_atmosphere_table() {
        // International Standard Atmosphere: 898.75 hPa at 1,000 m,
        // 701.09 hPa at 3,000 m, 540.20 hPa at 5,000 m.
        for (hpa, metres) in [
            (1013.25, 0.0),
            (898.75, 1000.0),
            (701.09, 3000.0),
            (540.20, 5000.0),
        ] {
            let altitude = pressure_altitude_m(hpa);
            assert!(
                (altitude - metres).abs() < 2.0,
                "{hpa} hPa gave {altitude} m"
            );
        }
    }
}
