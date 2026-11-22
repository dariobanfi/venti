//! Foehn, read from the pressure on both sides of the Alps' main ridge.
//!
//! Air flows over the ridge from the side with the higher pressure to the
//! side with the lower, and falls into the valleys there as foehn: warm,
//! dry, gusty and turbulent, the Alps' most dangerous wind for a
//! paraglider. The classic sign is the difference of the pressure, reduced
//! to sea level, between a station south and a station north of the ridge.
//! With the higher pressure in the south it is south foehn, which blows in
//! the valleys north of the ridge; with the higher pressure in the north,
//! north foehn, which blows south of it.
//!
//! Where the pairs and the limits are from (read on 2026-11-22):
//!
//! - **Bolzano and Innsbruck, 3 and 6 hPa.** The weather service of the
//!   Province of Bolzano publishes a foehn chart of exactly this difference
//!   and writes with it: with the lower pressure in Bolzano, north foehn in
//!   South Tyrol, "at least a pressure difference of 3 hPa for the
//!   breakthrough of foehn winds in the northern valleys like the Vinschgau
//!   and Wipptal and at least 6 hPa for the breakthrough in the southern
//!   region like Bolzano"; with the higher pressure in Bolzano, south foehn
//!   on the northern side of the Alps; and the foehn grows with the
//!   difference (<https://weather.provinz.bz.it/foehn-chart.asp>).
//! - **The same limits for south foehn** are *to verify with pilots*: the
//!   province gives figures for north foehn only. Deutscher Wetterdienst
//!   names 2 hPa between Locarno-Monti and Vaduz as the least for a foehn
//!   to reach the Alpine Rhine valley, another pair in another valley
//!   (<https://www.dwd.de/DE/wetter/thema_des_tages/2019/12/22.html>). So
//!   3 hPa is not early for "possible", and less than 3 hPa never reads as
//!   "no foehn" ([`Level::None`] is "no sign in the pressure").
//! - **Klagenfurt and Salzburg** is our own choice for the ridge of the
//!   Tauern, two airfields at nearly one height on either side, and *to
//!   verify with pilots*, with the same limits. No page of GeoSphere
//!   Austria, MeteoSwiss or Deutscher Wetterdienst that could be read on
//!   that day names a pair or a limit inside the first region.
//!
//! The pressure has to be at one level to be compared. A provider's own
//! reduction to sea level is taken where it sends one; else the pressure at
//! the station is reduced here ([`reduce_to_sea_level`]), which a cold
//! valley floor makes wrong by a hectopascal or more. So the figures are
//! whole hectopascals, and the level is chosen from the figure as shown.
//!
//! A difference says that air is pushed over the ridge. It does not say
//! that the foehn has reached a valley's floor, nor that it has not: foehn
//! blows with small differences, and a lake of cold air can keep it aloft
//! with large ones. So this informs and judges no day. A pair without two
//! readings of the last hour has no level at all ([`Judgement::Unknown`]),
//! which is never "no foehn".
//!
//! No I/O and no clock: the caller passes the readings and the time.

/// From this difference, in whole hectopascals, foehn is possible.
pub const POSSIBLE_HPA: f64 = 3.0;
/// From this difference, in whole hectopascals, foehn is likely.
pub const LIKELY_HPA: f64 = 6.0;
/// How old a reading may be to stand for the pressure now, seconds. An
/// hour, as on the wind map.
pub const FRESH_S: i64 = 3600;
/// How far apart in time two readings may be to be compared, seconds. The
/// pressure moves by a hectopascal in an hour on a lively day; half an
/// hour takes in stations that report every thirty minutes. Ours.
pub const TOGETHER_S: i64 = 30 * 60;
/// Up to this height a station's pressure is reduced to sea level here,
/// metres. Over a taller column the guess at its temperature weighs more
/// than a foehn's few hectopascals. Ours, and to verify.
pub const REDUCED_UP_TO_M: f64 = 1000.0;

/// A side of the main ridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// South of the ridge.
    South,
    /// North of the ridge.
    North,
}

/// A station of a pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairStation {
    /// The station's identifier, `source:local`.
    pub id: &'static str,
    /// The place's name as pilots say it, for a sentence.
    pub name: &'static str,
    /// Where it stands, degrees north.
    pub lat: f64,
    /// Where it stands, degrees east.
    pub lon: f64,
}

/// Two stations on either side of the main ridge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    /// A stable name for the pair.
    pub id: &'static str,
    /// The station south of the ridge.
    pub south: PairStation,
    /// The station north of the ridge.
    pub north: PairStation,
    /// Whether the pair or its limits are not from a weather service's own
    /// page and wait for pilots to confirm them.
    pub to_verify: bool,
    /// Where the pair and its limits are from, in a sentence.
    pub basis: &'static str,
}

/// The pairs of the first region. Each station is one the region collects
/// and that sends its pressure: Bolzano reduced to sea level by the
/// province, the Austrian stations at their own height.
pub const PAIRS: [Pair; 2] = [
    Pair {
        id: "bolzano-innsbruck",
        south: PairStation {
            id: "suedtirol-meteo:83200MS",
            name: "Bolzano",
            lat: 46.4977,
            lon: 11.3128,
        },
        north: PairStation {
            id: "geosphere-tawes:11320",
            name: "Innsbruck",
            lat: 47.26,
            lon: 11.3842,
        },
        to_verify: false,
        basis: "The pair and the limits of 3 and 6 hPa are those of the foehn chart of the \
                weather service of the Province of Bolzano, which gives them for north foehn; \
                for south foehn Venti uses the same limits, which no weather service has given.",
    },
    Pair {
        id: "klagenfurt-salzburg",
        south: PairStation {
            id: "geosphere-tawes:11331",
            name: "Klagenfurt",
            lat: 46.6483,
            lon: 14.3183,
        },
        north: PairStation {
            id: "geosphere-tawes:11150",
            name: "Salzburg",
            lat: 47.7894,
            lon: 13.0086,
        },
        to_verify: true,
        basis: "Venti's own choice of two airfields on either side of the Tauern, with the \
                limits of the pair Bolzano and Innsbruck. No weather service has given either.",
    },
];

/// What a station sent at one time, as far as the pressure needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StationReading {
    /// End of the measuring interval, seconds since 1970-01-01 UTC.
    pub time_unix_s: i64,
    /// The pressure at the station's own height, hPa.
    pub pressure_hpa: Option<f64>,
    /// The pressure reduced to sea level by the provider, hPa.
    pub pressure_sea_level_hpa: Option<f64>,
    /// The air temperature at the station, °C.
    pub temperature_c: Option<f64>,
}

/// A station's pressure at sea level at one time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeaLevelPressure {
    /// End of the measuring interval, seconds since 1970-01-01 UTC.
    pub time_unix_s: i64,
    /// The pressure at sea level, hPa.
    pub hpa: f64,
    /// Whether it was reduced here from the pressure at the station, and
    /// not by the provider.
    pub reduced_here: bool,
}

/// The pressure at sea level under a station, from the pressure at the
/// station, its height and its temperature, hPa.
///
/// The barometric formula for a column of air whose temperature rises
/// downwards by 0.65 °C in 100 m, the standard atmosphere's step, and is
/// taken at the column's middle:
/// `p0 = p · exp(g · h / (R · (T + 0.00325 · h)))`, with `g` = 9.80665
/// m/s², `R` = 287.05 J/(kg·K) for dry air and `T` in kelvin. The weather
/// services add the air's moisture and smooth the temperature over twelve
/// hours (to verify against DWD's and GeoSphere's own practice); this does
/// neither, and reads too high over a cold valley floor.
#[must_use]
pub fn reduce_to_sea_level(pressure_hpa: f64, elevation_m: f64, temperature_c: f64) -> f64 {
    const GRAVITY: f64 = 9.806_65;
    const GAS_CONSTANT: f64 = 287.05;
    const HALF_LAPSE_K_PER_M: f64 = 0.003_25;
    let mean_k = temperature_c + 273.15 + HALF_LAPSE_K_PER_M * elevation_m;
    pressure_hpa * (GRAVITY * elevation_m / (GAS_CONSTANT * mean_k)).exp()
}

/// A reading's pressure at sea level: the provider's own where it sends
/// one, else reduced here from the pressure at the station. `None` when
/// the reading has no pressure, or has the station's only and the height
/// or the temperature to reduce it lacks, or the station stands above
/// [`REDUCED_UP_TO_M`].
#[must_use]
pub fn at_sea_level(
    reading: &StationReading,
    elevation_m: Option<f64>,
) -> Option<SeaLevelPressure> {
    if let Some(hpa) = reading.pressure_sea_level_hpa {
        return Some(SeaLevelPressure {
            time_unix_s: reading.time_unix_s,
            hpa,
            reduced_here: false,
        });
    }
    let elevation_m = elevation_m.filter(|height| (0.0..=REDUCED_UP_TO_M).contains(height))?;
    Some(SeaLevelPressure {
        time_unix_s: reading.time_unix_s,
        hpa: reduce_to_sea_level(reading.pressure_hpa?, elevation_m, reading.temperature_c?),
        reduced_here: true,
    })
}

/// The pressure in the south less the pressure in the north at one time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Difference {
    /// The time of the older of the two readings, seconds since 1970-01-01
    /// UTC.
    pub time_unix_s: i64,
    /// South less north, hPa, to a tenth. Above zero the south is higher.
    pub hpa: f64,
}

/// The difference of two readings, if they are close enough in time to be
/// compared ([`TOGETHER_S`]).
#[must_use]
pub fn difference(south: &SeaLevelPressure, north: &SeaLevelPressure) -> Option<Difference> {
    ((south.time_unix_s - north.time_unix_s).abs() <= TOGETHER_S).then(|| Difference {
        time_unix_s: south.time_unix_s.min(north.time_unix_s),
        hpa: ((south.hpa - north.hpa) * 10.0).round() / 10.0,
    })
}

/// What a difference says about foehn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// No sign of foehn in the pressure. Not "no foehn".
    None,
    /// Foehn is possible.
    Possible,
    /// Foehn is likely.
    Likely,
}

/// The figure a difference is shown as: whole hectopascals, without a
/// sign. Halves go up, towards the warning.
#[must_use]
pub fn shown_hpa(difference_hpa: f64) -> f64 {
    (difference_hpa.abs() + 0.5).floor()
}

/// The level of a difference, chosen from the figure as shown
/// ([`shown_hpa`]), so that the words and the level cannot disagree.
#[must_use]
pub fn level(difference_hpa: f64) -> Level {
    let shown = shown_hpa(difference_hpa);
    if shown >= LIKELY_HPA {
        Level::Likely
    } else if shown >= POSSIBLE_HPA {
        Level::Possible
    } else {
        Level::None
    }
}

/// The side with the higher pressure, by the figure as shown: neither when
/// that is zero.
#[must_use]
pub fn higher(difference_hpa: f64) -> Option<Side> {
    if shown_hpa(difference_hpa) < 1.0 {
        None
    } else if difference_hpa > 0.0 {
        Some(Side::South)
    } else {
        Some(Side::North)
    }
}

/// Why a pair has no level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unknown {
    /// A station has no pressure that can be compared.
    Missing(Side),
    /// A station's newest pressure is older than [`FRESH_S`].
    Old(Side),
    /// The two newest readings are further apart than [`TOGETHER_S`].
    Apart,
}

/// What a pair's newest readings say.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Judgement {
    /// Both stations have a reading of the last hour.
    Known {
        /// South less north.
        difference: Difference,
        /// The side with the higher pressure, if either.
        higher: Option<Side>,
        /// What that says about foehn. At [`Level::Possible`] and above it
        /// is south foehn with the south higher and north foehn with the
        /// north higher.
        level: Level,
    },
    /// Nothing can be said, and why. Never "no foehn".
    Unknown(Unknown),
}

/// Judges a pair by the newest pressure of each station at `now_unix_s`.
///
/// A reading older than an hour or missing gives no level. A reading
/// stamped after `now_unix_s` counts as fresh: the clocks of a server and
/// a provider differ by minutes.
#[must_use]
pub fn judge(
    south: Option<&SeaLevelPressure>,
    north: Option<&SeaLevelPressure>,
    now_unix_s: i64,
) -> Judgement {
    let Some(south) = south else {
        return Judgement::Unknown(Unknown::Missing(Side::South));
    };
    let Some(north) = north else {
        return Judgement::Unknown(Unknown::Missing(Side::North));
    };
    if now_unix_s - south.time_unix_s > FRESH_S {
        return Judgement::Unknown(Unknown::Old(Side::South));
    }
    if now_unix_s - north.time_unix_s > FRESH_S {
        return Judgement::Unknown(Unknown::Old(Side::North));
    }
    judge_together(south, north)
}

/// Judges two readings without asking how old they are: for a reader who
/// does not know the time, and says the readings' own with the answer.
#[must_use]
pub fn judge_together(south: &SeaLevelPressure, north: &SeaLevelPressure) -> Judgement {
    match difference(south, north) {
        Some(difference) => Judgement::Known {
            difference,
            higher: higher(difference.hpa),
            level: level(difference.hpa),
        },
        None => Judgement::Unknown(Unknown::Apart),
    }
}

/// The differences over time: for each reading of the south the reading of
/// the north nearest to it in time, if one is near enough to compare, the
/// oldest first. Both lists are oldest first.
#[must_use]
pub fn series(south: &[SeaLevelPressure], north: &[SeaLevelPressure]) -> Vec<Difference> {
    let mut found = Vec::new();
    let mut from = 0;
    for reading in south {
        // The north's readings before this one cannot be nearer to a later
        // reading of the south than the one found here.
        while from + 1 < north.len()
            && (north[from + 1].time_unix_s - reading.time_unix_s).abs()
                <= (north[from].time_unix_s - reading.time_unix_s).abs()
        {
            from += 1;
        }
        if let Some(difference) = north.get(from).and_then(|north| difference(reading, north)) {
            found.push(difference);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_795_028_700;

    fn at(minutes_ago: i64, hpa: f64) -> SeaLevelPressure {
        SeaLevelPressure {
            time_unix_s: NOW - minutes_ago * 60,
            hpa,
            reduced_here: false,
        }
    }

    fn known(south: f64, north: f64) -> (f64, Option<Side>, Level) {
        match judge(Some(&at(10, south)), Some(&at(5, north)), NOW) {
            Judgement::Known {
                difference,
                higher,
                level,
            } => (difference.hpa, higher, level),
            Judgement::Unknown(why) => panic!("no judgement: {why:?}"),
        }
    }

    #[test]
    fn each_level_in_each_direction() {
        assert_eq!(known(1016.0, 1015.8), (0.2, None, Level::None));
        assert_eq!(known(1017.0, 1015.0), (2.0, Some(Side::South), Level::None));
        assert_eq!(
            known(1018.0, 1015.0),
            (3.0, Some(Side::South), Level::Possible)
        );
        assert_eq!(
            known(1020.4, 1015.0),
            (5.4, Some(Side::South), Level::Possible)
        );
        assert_eq!(
            known(1021.2, 1015.0),
            (6.2, Some(Side::South), Level::Likely)
        );
        assert_eq!(
            known(1012.0, 1014.0),
            (-2.0, Some(Side::North), Level::None)
        );
        assert_eq!(
            known(1011.0, 1015.0),
            (-4.0, Some(Side::North), Level::Possible)
        );
        assert_eq!(
            known(1006.0, 1015.0),
            (-9.0, Some(Side::North), Level::Likely)
        );
    }

    #[test]
    fn the_level_is_chosen_from_the_figure_as_shown() {
        // 2.5 is shown as 3 and 5.5 as 6: halves go towards the warning.
        assert_eq!(level(2.4), Level::None);
        assert_eq!(level(2.5), Level::Possible);
        assert_eq!(level(-5.4), Level::Possible);
        assert_eq!(level(-5.5), Level::Likely);
        assert_eq!(higher(0.4), None);
        assert_eq!(higher(-0.5), Some(Side::North));
    }

    #[test]
    fn a_stale_or_missing_reading_gives_no_level() {
        let fresh = at(5, 1015.0);
        // Nine hectopascals would be "likely": an old reading says nothing.
        let old = at(61, 1024.0);
        assert_eq!(
            judge(Some(&old), Some(&fresh), NOW),
            Judgement::Unknown(Unknown::Old(Side::South))
        );
        assert_eq!(
            judge(Some(&fresh), Some(&old), NOW),
            Judgement::Unknown(Unknown::Old(Side::North))
        );
        assert_eq!(
            judge(None, Some(&fresh), NOW),
            Judgement::Unknown(Unknown::Missing(Side::South))
        );
        assert_eq!(
            judge(Some(&fresh), None, NOW),
            Judgement::Unknown(Unknown::Missing(Side::North))
        );
        // Exactly an hour old still counts, as on the wind map.
        assert!(matches!(
            judge(Some(&at(60, 1024.0)), Some(&at(40, 1015.0)), NOW),
            Judgement::Known {
                level: Level::Likely,
                ..
            }
        ));
        // Both of the last hour, and too far apart to be compared.
        assert_eq!(
            judge(Some(&at(55, 1024.0)), Some(&fresh), NOW),
            Judgement::Unknown(Unknown::Apart)
        );
        // A provider's clock a few minutes ahead of ours.
        assert!(matches!(
            judge(Some(&at(-3, 1016.0)), Some(&fresh), NOW),
            Judgement::Known { .. }
        ));
    }

    #[test]
    fn the_reduction_to_sea_level_against_a_worked_example() {
        // Innsbruck's university on 18 November 2026: 930.9 hPa at 578 m
        // and 7.8 °C. The column's mean temperature is 280.95 K + 0.00325
        // K/m · 578 m = 282.8285 K; g·h/(R·T) = 9.80665 · 578 /
        // (287.05 · 282.8285) = 0.069818; e to that is 1.072313; and
        // 930.9 hPa · 1.072313 = 998.2 hPa.
        let reduced = reduce_to_sea_level(930.9, 578.0, 7.8);
        assert!((reduced - 998.2).abs() < 0.05, "{reduced}");
        // At sea level nothing is added, and colder air weighs more.
        assert!((reduce_to_sea_level(1013.2, 0.0, 15.0) - 1013.2).abs() < 1e-9);
        assert!(reduce_to_sea_level(930.9, 578.0, -5.0) > reduced + 2.0);
    }

    #[test]
    fn a_providers_reduction_comes_first_and_ours_needs_height_and_temperature() {
        let mut reading = StationReading {
            time_unix_s: NOW,
            pressure_hpa: Some(930.9),
            pressure_sea_level_hpa: Some(1001.0),
            temperature_c: Some(7.8),
        };
        let theirs = at_sea_level(&reading, Some(578.0)).unwrap();
        assert!(!theirs.reduced_here);
        assert!((theirs.hpa - 1001.0).abs() < 1e-9);

        reading.pressure_sea_level_hpa = None;
        let ours = at_sea_level(&reading, Some(578.0)).unwrap();
        assert!(ours.reduced_here);
        assert!((ours.hpa - 998.2).abs() < 0.05);

        // Without the height, above the limit, or without the temperature
        // there is no pressure to compare.
        assert_eq!(at_sea_level(&reading, None), None);
        assert_eq!(at_sea_level(&reading, Some(2251.0)), None);
        reading.temperature_c = None;
        assert_eq!(at_sea_level(&reading, Some(578.0)), None);
    }

    #[test]
    fn the_series_pairs_each_reading_with_the_nearest_in_time() {
        let south = [at(120, 1020.0), at(60, 1021.0), at(10, 1022.0)];
        // Nothing near the first; two near the second, the nearer taken.
        let north = [at(75, 1010.0), at(58, 1016.0), at(5, 1015.0)];
        let found = series(&south, &north);
        assert_eq!(
            found
                .iter()
                .map(|d| ((NOW - d.time_unix_s) / 60, d.hpa))
                .collect::<Vec<_>>(),
            vec![(60, 5.0), (10, 7.0)]
        );
        assert!(series(&south, &[]).is_empty());
        assert!(series(&[], &north).is_empty());
    }

    #[test]
    fn the_pairs_have_a_station_on_either_side() {
        for pair in PAIRS {
            assert!(pair.south.lat < pair.north.lat, "{}", pair.id);
            assert!(pair.south.id.contains(':') && pair.north.id.contains(':'));
        }
    }
}
