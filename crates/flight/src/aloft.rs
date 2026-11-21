//! The wind at a height above the sea, read between a model's levels.
//!
//! A model gives its wind at levels of its own: 10 m above its ground, a
//! few fixed heights above that, and surfaces of equal pressure, whose
//! heights above the sea come with them and change with the weather. A
//! pilot asks for the wind "at 2000 m". [`wind_at_height`] reads it from
//! the two levels the height lies between, and says so plainly where it
//! cannot: under the model's ground, and above its highest level. It never
//! reads beyond the levels there are.
//!
//! How it reads between two levels is our choice: the speed in a straight
//! line by height, the direction along the shorter way round the compass.
//! The speed is then never below both levels', which the other common way,
//! a straight line for the wind's east and north parts, is where the two
//! levels blow against each other. The levels can lie far apart: between
//! the surfaces of 850 and 700 hPa are some 1500 m, and what the wind does
//! in between, a shear or an inversion's own wind, the reading does not
//! know. To verify against soundings.

use venti_domain::ForecastHour;

/// How far above its ground a model's lowest wind is, metres.
pub const LOWEST_WIND_M: f64 = 10.0;

/// A wind at a height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindAt {
    /// Its speed, m/s.
    pub speed_ms: f64,
    /// The direction it comes from, degrees clockwise from north, from 0
    /// to below 360. `None` where the model gives none.
    pub from_deg: Option<f64>,
}

/// What a model's hour says of the wind at a height above the sea.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AtHeight {
    /// The wind there, read at a level or between two.
    Wind(WindAt),
    /// The height lies under the model's ground.
    UnderGround,
    /// The height lies above the highest level the model gives a wind at.
    AboveLevels,
    /// The model gives nothing to read it from: no height of its ground,
    /// no wind at any level, or none at a level below the height.
    Unknown,
}

impl AtHeight {
    /// The wind, where there is one.
    #[must_use]
    pub fn wind(self) -> Option<WindAt> {
        match self {
            Self::Wind(wind) => Some(wind),
            Self::UnderGround | Self::AboveLevels | Self::Unknown => None,
        }
    }
}

/// The wind an hour of a model has at `height_m` above mean sea level,
/// over a cell whose ground the model has at `ground_m`.
///
/// The levels it reads from are the wind 10 m above the ground, the
/// levels at a fixed height above the ground, and the pressure surfaces
/// that come with a height and lie above the ground: a surface under the
/// model's ground has no air on it, whatever the model writes there. A
/// height from the ground up to the lowest wind, 10 m, reads that wind.
/// Without the height of the ground there is nothing to place the levels
/// by, and the answer is [`AtHeight::Unknown`].
#[must_use]
pub fn wind_at_height(hour: &ForecastHour, ground_m: Option<f64>, height_m: f64) -> AtHeight {
    let Some(ground) = ground_m else {
        return AtHeight::Unknown;
    };
    if height_m < ground {
        return AtHeight::UnderGround;
    }
    let mut levels: Vec<(f64, WindAt)> = Vec::with_capacity(hour.winds_aloft.len() + 1);
    if let Some(speed_ms) = hour.wind_speed_ms {
        levels.push((
            ground + LOWEST_WIND_M,
            WindAt {
                speed_ms,
                from_deg: hour.wind_direction_deg.map(compass),
            },
        ));
    }
    for level in &hour.winds_aloft {
        let altitude = match (level.above_ground_m, level.altitude_m) {
            (Some(above), _) => Some(ground + above),
            (None, Some(altitude)) if altitude >= ground => Some(altitude),
            _ => None,
        };
        if let (Some(altitude), Some(speed_ms)) = (altitude, level.wind_speed_ms) {
            levels.push((
                altitude,
                WindAt {
                    speed_ms,
                    from_deg: level.wind_direction_deg.map(compass),
                },
            ));
        }
    }
    levels.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (Some(lowest), Some(highest)) = (levels.first(), levels.last()) else {
        return AtHeight::Unknown;
    };
    if height_m > highest.0 {
        return AtHeight::AboveLevels;
    }
    if height_m <= lowest.0 {
        // Between the ground and the lowest wind there is: that wind, if
        // it is the one at the ground.
        return if lowest.0 - ground <= LOWEST_WIND_M {
            AtHeight::Wind(lowest.1)
        } else {
            AtHeight::Unknown
        };
    }
    let above = levels
        .iter()
        .position(|(altitude, _)| *altitude >= height_m)
        .unwrap_or(levels.len() - 1);
    let (below_m, below) = levels[above.saturating_sub(1)];
    let (above_m, above) = levels[above];
    let span = above_m - below_m;
    if span <= 0.0 {
        return AtHeight::Wind(above);
    }
    let share = ((height_m - below_m) / span).clamp(0.0, 1.0);
    AtHeight::Wind(WindAt {
        speed_ms: below.speed_ms + share * (above.speed_ms - below.speed_ms),
        from_deg: match (below.from_deg, above.from_deg) {
            (Some(from), Some(to)) => Some(between(from, to, share)),
            // One level without a direction: the other's.
            (from, to) => from.or(to),
        },
    })
}

/// A direction as a compass has it: from 0 to below 360.
fn compass(degrees: f64) -> f64 {
    let turned = degrees.rem_euclid(360.0);
    // A hair under zero comes out of the remainder as a full turn.
    if turned >= 360.0 { 0.0 } else { turned }
}

/// The direction a `share` of the way from one to another, along the
/// shorter way round. Two that are opposite turn clockwise.
fn between(from: f64, to: f64, share: f64) -> f64 {
    let mut turn = (to - from).rem_euclid(360.0);
    if turn > 180.0 {
        turn -= 360.0;
    }
    compass(from + share * turn)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::WindAloft;

    use super::*;

    /// An hour over ground at 800 m: 2 m/s from 200° at 10 m, levels 80
    /// and 180 m above the ground, and the surfaces of 950 hPa (under the
    /// ground, at 540 m), 850 hPa at 1500 m and 700 hPa at 3000 m.
    fn hour() -> ForecastHour {
        let mut hour = ForecastHour::empty(Utc.with_ymd_and_hms(2026, 7, 12, 12, 0, 0).unwrap());
        hour.wind_speed_ms = Some(2.0);
        hour.wind_direction_deg = Some(200.0);
        let above = |metres: f64, speed: f64, from: f64| {
            let mut level = WindAloft::above_ground(metres);
            level.wind_speed_ms = Some(speed);
            level.wind_direction_deg = Some(from);
            level
        };
        let surface = |hpa: f64, altitude: f64, speed: f64, from: f64| {
            let mut level = WindAloft::at_pressure(hpa);
            level.altitude_m = Some(altitude);
            level.wind_speed_ms = Some(speed);
            level.wind_direction_deg = Some(from);
            level
        };
        hour.winds_aloft = vec![
            above(80.0, 3.0, 210.0),
            above(180.0, 4.0, 220.0),
            surface(950.0, 540.0, 30.0, 90.0),
            surface(850.0, 1500.0, 6.0, 270.0),
            surface(700.0, 3000.0, 12.0, 300.0),
        ];
        hour
    }

    fn wind(found: AtHeight) -> (f64, f64) {
        let wind = found.wind().expect("a wind");
        (wind.speed_ms, wind.from_deg.expect("a direction"))
    }

    fn close(found: (f64, f64), speed: f64, from: f64) {
        assert!(
            (found.0 - speed).abs() < 1e-9 && (found.1 - from).abs() < 1e-9,
            "{found:?} is not {speed} from {from}"
        );
    }

    #[test]
    fn a_height_between_two_levels_is_read_in_a_straight_line() {
        let hour = hour();
        // Half way between 850 hPa at 1500 m and 700 hPa at 3000 m.
        close(wind(wind_at_height(&hour, Some(800.0), 2250.0)), 9.0, 285.0);
        // A third of the way.
        close(wind(wind_at_height(&hour, Some(800.0), 2000.0)), 8.0, 280.0);
        // At a level, the level's wind.
        close(wind(wind_at_height(&hour, Some(800.0), 1500.0)), 6.0, 270.0);
        close(
            wind(wind_at_height(&hour, Some(800.0), 3000.0)),
            12.0,
            300.0,
        );
        // Between the levels above the ground, which lie at 880 and 980 m.
        close(wind(wind_at_height(&hour, Some(800.0), 930.0)), 3.5, 215.0);
        // And between the highest of them and the first surface above.
        close(wind(wind_at_height(&hour, Some(800.0), 1240.0)), 5.0, 245.0);
    }

    #[test]
    fn a_height_under_the_ground_has_no_wind_and_says_so() {
        let hour = hour();
        assert_eq!(
            wind_at_height(&hour, Some(800.0), 799.0),
            AtHeight::UnderGround
        );
        assert_eq!(
            wind_at_height(&hour, Some(1240.0), 1000.0),
            AtHeight::UnderGround
        );
        assert_eq!(AtHeight::UnderGround.wind(), None);
        // The surface of 950 hPa lies under this ground: its wind, 30 m/s
        // from the east, is no part of any reading.
        close(wind(wind_at_height(&hour, Some(800.0), 805.0)), 2.0, 200.0);
        // From the ground up to 10 m the wind is the one at 10 m.
        close(wind(wind_at_height(&hour, Some(800.0), 800.0)), 2.0, 200.0);
        close(wind(wind_at_height(&hour, Some(800.0), 810.0)), 2.0, 200.0);
        // Over lower ground the same surface is a level like any other.
        let (speed, _) = wind(wind_at_height(&hour, Some(300.0), 540.0));
        assert!((speed - 30.0).abs() < 1e-9);
    }

    #[test]
    fn a_height_above_the_highest_level_has_no_wind_and_says_so() {
        let hour = hour();
        assert_eq!(
            wind_at_height(&hour, Some(800.0), 3001.0),
            AtHeight::AboveLevels
        );
        assert_eq!(
            wind_at_height(&hour, Some(800.0), 4000.0),
            AtHeight::AboveLevels
        );
        // A surface that comes without a wind is no level.
        let mut calm_top = hour.clone();
        calm_top.winds_aloft[4].wind_speed_ms = None;
        assert_eq!(
            wind_at_height(&calm_top, Some(800.0), 2000.0),
            AtHeight::AboveLevels
        );
    }

    #[test]
    fn a_direction_turns_the_shorter_way_across_north() {
        let mut hour = ForecastHour::empty(Utc.with_ymd_and_hms(2026, 7, 12, 12, 0, 0).unwrap());
        hour.wind_speed_ms = Some(4.0);
        hour.wind_direction_deg = Some(350.0);
        let mut top = WindAloft::above_ground(110.0);
        top.wind_speed_ms = Some(8.0);
        top.wind_direction_deg = Some(30.0);
        hour.winds_aloft = vec![top];
        // From 350° to 30° is 40° through north, not 320° through south.
        close(wind(wind_at_height(&hour, Some(0.0), 35.0)), 5.0, 0.0);
        close(wind(wind_at_height(&hour, Some(0.0), 60.0)), 6.0, 10.0);
        close(wind(wind_at_height(&hour, Some(0.0), 22.5)), 4.5, 355.0);
        // And the other way round.
        hour.wind_direction_deg = Some(30.0);
        hour.winds_aloft[0].wind_direction_deg = Some(350.0);
        close(wind(wind_at_height(&hour, Some(0.0), 85.0)), 7.0, 0.0);
        // Opposite winds: the speed is never below both levels'.
        hour.wind_direction_deg = Some(90.0);
        hour.winds_aloft[0].wind_direction_deg = Some(270.0);
        let (speed, from) = wind(wind_at_height(&hour, Some(0.0), 60.0));
        assert!((speed - 6.0).abs() < 1e-9 && (from - 180.0).abs() < 1e-9);
        // A direction given as 360 or below zero is a compass's.
        hour.wind_direction_deg = Some(360.0);
        hour.winds_aloft[0].wind_direction_deg = Some(-20.0);
        close(wind(wind_at_height(&hour, Some(0.0), 60.0)), 6.0, 350.0);
    }

    #[test]
    fn without_a_ground_or_a_level_nothing_is_read() {
        let hour = hour();
        // A cell of water: the model gives no height of its ground.
        assert_eq!(wind_at_height(&hour, None, 2000.0), AtHeight::Unknown);
        // An hour without any wind.
        let empty = ForecastHour::empty(hour.time);
        assert_eq!(
            wind_at_height(&empty, Some(800.0), 2000.0),
            AtHeight::Unknown
        );
        // No wind at the ground, and the height lies between the ground
        // and the lowest level there is: nothing below to read from.
        let mut no_ground_wind = hour.clone();
        no_ground_wind.wind_speed_ms = None;
        assert_eq!(
            wind_at_height(&no_ground_wind, Some(800.0), 840.0),
            AtHeight::Unknown
        );
        close(
            wind(wind_at_height(&no_ground_wind, Some(800.0), 930.0)),
            3.5,
            215.0,
        );
        // A level without a direction takes the other's; the speed is
        // read as ever.
        let mut one_direction = hour.clone();
        one_direction.winds_aloft[4].wind_direction_deg = None;
        close(
            wind(wind_at_height(&one_direction, Some(800.0), 2250.0)),
            9.0,
            270.0,
        );
    }
}
