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

#[cfg(test)]
mod tests {
    use super::*;

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
