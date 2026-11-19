//! When the sun sets.
//!
//! A paraglider has to be on the ground before dark, and in the Alps in
//! autumn that comes early. This works out sunrise, sunset and the ends of
//! civil twilight for a place and a day, without tables and without a
//! network.
//!
//! The method is the one of NOAA's solar calculator, after Jean Meeus'
//! "Astronomical Algorithms": the sun's place in the sky from the date,
//! then the moment its centre is 0.833 degrees below a level horizon
//! (which allows for its radius and for the bending of light in the air),
//! and 6 degrees below for civil twilight. It is good to about a minute
//! in the latitudes people fly in. The tests compare it with the US Naval
//! Observatory's figures.
//!
//! The horizon is taken as level at sea level. In a valley the sun is
//! gone behind the mountains long before this sunset; on a summit it is
//! seen a little longer. The figure is the legal and practical end of the
//! day, not when the light goes at one's landing field.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use venti_domain::GeoPoint;

/// The sun's centre this far below the horizon marks sunrise and sunset.
const SUNSET_ZENITH_DEG: f64 = 90.833;

/// And this far marks the ends of civil twilight.
const CIVIL_ZENITH_DEG: f64 = 96.0;

/// The light of one day at one place. A time is `None` when the event
/// does not happen that day: north of the polar circle in summer the sun
/// does not set, and in winter it does not rise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Daylight {
    /// The beginning of civil twilight in the morning.
    pub civil_dawn: Option<DateTime<Utc>>,
    /// Sunrise.
    pub sunrise: Option<DateTime<Utc>>,
    /// Sunset.
    pub sunset: Option<DateTime<Utc>>,
    /// The end of civil twilight in the evening. From here on it is night.
    pub civil_dusk: Option<DateTime<Utc>>,
}

/// The light of the day that is under way at a place at a moment: the day
/// by the sun there, from one midnight by the sun to the next, wherever
/// the date line and Greenwich are.
pub fn daylight_at(time: DateTime<Utc>, position: GeoPoint) -> Daylight {
    // The sun's own clock at this longitude: four minutes a degree.
    let local = time + Duration::seconds((position.lon() * 240.0).round() as i64);
    daylight(local.date_naive(), position)
}

/// The light of a day at a place. `date` is the day by the sun at the
/// place; the times returned are UTC and may fall on a neighbouring date.
pub fn daylight(date: NaiveDate, position: GeoPoint) -> Daylight {
    let midnight = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap_or_default());
    // Centuries since noon on 1 January 2000, taken at the day's noon.
    let days = (midnight - Utc.with_ymd_and_hms(2000, 1, 1, 12, 0, 0).unwrap()).num_seconds()
        as f64
        / 86_400.0
        + 0.5;
    let century = days / 36_525.0;
    let sun = Sun::at(century);

    // Solar noon in minutes after midnight UTC of `date`.
    let noon_min = 720.0 - 4.0 * position.lon() - sun.equation_of_time_min;
    let moment = |minutes: f64| midnight + Duration::seconds((minutes * 60.0).round() as i64);
    let event = |zenith_deg: f64, sign: f64| {
        hour_angle_deg(position.lat(), sun.declination_deg, zenith_deg)
            .map(|angle| moment(noon_min + sign * 4.0 * angle))
    };
    Daylight {
        civil_dawn: event(CIVIL_ZENITH_DEG, -1.0),
        sunrise: event(SUNSET_ZENITH_DEG, -1.0),
        sunset: event(SUNSET_ZENITH_DEG, 1.0),
        civil_dusk: event(CIVIL_ZENITH_DEG, 1.0),
    }
}

/// Where the sun stands, as far as rising and setting need it.
struct Sun {
    declination_deg: f64,
    /// How far a sundial is ahead of a clock, minutes.
    equation_of_time_min: f64,
}

impl Sun {
    /// The sun at a moment given in Julian centuries since 2000.
    fn at(t: f64) -> Self {
        let mean_longitude = (280.466_46 + t * (36_000.769_83 + 0.000_303_2 * t)).rem_euclid(360.0);
        let mean_anomaly = 357.529_11 + t * (35_999.050_29 - 0.000_153_7 * t);
        let eccentricity = 0.016_708_634 - t * (0.000_042_037 + 0.000_000_126_7 * t);
        let (m, l) = (mean_anomaly.to_radians(), mean_longitude.to_radians());
        let centre = m.sin() * (1.914_602 - t * (0.004_817 + 0.000_014 * t))
            + (2.0 * m).sin() * (0.019_993 - 0.000_101 * t)
            + (3.0 * m).sin() * 0.000_289;
        let omega = (125.04 - 1934.136 * t).to_radians();
        let apparent_longitude =
            (mean_longitude + centre - 0.005_69 - 0.004_78 * omega.sin()).to_radians();
        let mean_obliquity =
            23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.000_59 - t * 0.001_813))) / 60.0) / 60.0;
        let obliquity = (mean_obliquity + 0.002_56 * omega.cos()).to_radians();

        let declination = (obliquity.sin() * apparent_longitude.sin()).asin();
        let y = (obliquity / 2.0).tan().powi(2);
        let equation_of_time = y * (2.0 * l).sin() - 2.0 * eccentricity * m.sin()
            + 4.0 * eccentricity * y * m.sin() * (2.0 * l).cos()
            - 0.5 * y * y * (4.0 * l).sin()
            - 1.25 * eccentricity * eccentricity * (2.0 * m).sin();
        Self {
            declination_deg: declination.to_degrees(),
            equation_of_time_min: 4.0 * equation_of_time.to_degrees(),
        }
    }
}

/// How far the sun is from the meridian, in degrees, when it stands at a
/// zenith distance. `None` when it never gets there that day.
fn hour_angle_deg(latitude_deg: f64, declination_deg: f64, zenith_deg: f64) -> Option<f64> {
    let (latitude, declination) = (latitude_deg.to_radians(), declination_deg.to_radians());
    let cosine = zenith_deg.to_radians().cos() / (latitude.cos() * declination.cos())
        - latitude.tan() * declination.tan();
    (-1.0..=1.0)
        .contains(&cosine)
        .then(|| cosine.acos().to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    /// Asserts a time of day in UTC, to within two minutes.
    fn assert_near(found: Option<DateTime<Utc>>, date: NaiveDate, hour: u32, minute: u32) {
        let expected = Utc.from_utc_datetime(&date.and_hms_opt(hour, minute, 0).unwrap());
        let found = found.expect("the event happens on this day");
        assert!(
            (found - expected).num_seconds().abs() <= 120,
            "{found} is not within two minutes of {expected}"
        );
    }

    /// The US Naval Observatory's figures (aa.usno.navy.mil, "Sun or Moon
    /// rise/set table for one day", fetched 2026-11-19), in UTC.
    #[test]
    fn the_times_agree_with_the_naval_observatory() {
        // Munich on the day this was written.
        let munich = GeoPoint::new(48.14, 11.58).unwrap();
        let light = daylight(day(2026, 11, 19), munich);
        assert_near(light.civil_dawn, day(2026, 11, 19), 5, 52);
        assert_near(light.sunrise, day(2026, 11, 19), 6, 26);
        assert_near(light.sunset, day(2026, 11, 19), 15, 31);
        assert_near(light.civil_dusk, day(2026, 11, 19), 16, 6);

        // The Zugspitze at midsummer.
        let zugspitze = GeoPoint::new(47.42, 10.98).unwrap();
        let light = daylight(day(2026, 6, 21), zugspitze);
        assert_near(light.civil_dawn, day(2026, 6, 21), 2, 39);
        assert_near(light.sunrise, day(2026, 6, 21), 3, 19);
        assert_near(light.sunset, day(2026, 6, 21), 19, 17);
        assert_near(light.civil_dusk, day(2026, 6, 21), 19, 57);

        // Innsbruck on the shortest day.
        let innsbruck = GeoPoint::new(47.26, 11.38).unwrap();
        let light = daylight(day(2026, 12, 21), innsbruck);
        assert_near(light.sunrise, day(2026, 12, 21), 6, 58);
        assert_near(light.sunset, day(2026, 12, 21), 15, 27);
        assert_near(light.civil_dusk, day(2026, 12, 21), 16, 2);

        // The Valais at the equinox.
        let valais = GeoPoint::new(46.5, 8.0).unwrap();
        let light = daylight(day(2026, 3, 20), valais);
        assert_near(light.civil_dawn, day(2026, 3, 20), 5, 1);
        assert_near(light.sunrise, day(2026, 3, 20), 5, 31);
        assert_near(light.sunset, day(2026, 3, 20), 17, 40);
        assert_near(light.civil_dusk, day(2026, 3, 20), 18, 11);
    }

    #[test]
    fn where_the_sun_does_not_set_there_is_no_sunset() {
        // Tromsø at midsummer: the sun stays up, and it never gets dark.
        let tromso = GeoPoint::new(69.65, 18.96).unwrap();
        let summer = daylight(day(2026, 6, 21), tromso);
        assert_eq!(summer.sunset, None);
        assert_eq!(summer.sunrise, None);
        assert_eq!(summer.civil_dusk, None);
        // And at midwinter it does not rise, though there is twilight.
        let winter = daylight(day(2026, 12, 21), tromso);
        assert_eq!(winter.sunrise, None);
        assert_eq!(winter.sunset, None);
        assert!(winter.civil_dawn.is_some() && winter.civil_dusk.is_some());
        // The poles do not upset anything.
        let _ = daylight(day(2026, 6, 21), GeoPoint::new(90.0, 0.0).unwrap());
        let _ = daylight(day(2026, 12, 21), GeoPoint::new(-90.0, 180.0).unwrap());
    }

    #[test]
    fn the_day_is_the_one_by_the_sun_at_the_place() {
        // Early afternoon in New Zealand is still the evening before in
        // UTC: the sunset asked for is the coming one there, not one that
        // is hours past or a day ahead.
        let wanaka = GeoPoint::new(-44.7, 169.1).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 1, 10, 1, 0, 0).unwrap();
        let sunset = daylight_at(now, wanaka).sunset.unwrap();
        let hours = (sunset - now).num_minutes() as f64 / 60.0;
        assert!((6.0..9.0).contains(&hours), "{hours} hours to sunset");

        // In the Alps the day by the sun is the calendar day.
        let munich = GeoPoint::new(48.14, 11.58).unwrap();
        let noon = Utc.with_ymd_and_hms(2026, 11, 19, 11, 0, 0).unwrap();
        assert_eq!(
            daylight_at(noon, munich),
            daylight(day(2026, 11, 19), munich)
        );
    }
}
