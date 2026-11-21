//! What a forecast makes of a day for flying.
//!
//! Three steps, each a fixed rule with its limits written here:
//!
//! - **An hour's verdict by the air alone** ([`against`], [`worst`]): rain,
//!   wind at the ground and higher up, gusts, a cloud base at or below the
//!   place, and air in which showers can grow each count against an hour.
//!   What a take-off adds to that (the directions it works in) is the
//!   caller's, who knows the take-off.
//! - **What an hour's thermals count as** ([`kind_of_hour`]): how strong
//!   they are, how far above the place they reach and how much wind blows
//!   through them.
//! - **The kind of day** ([`thermal_day`], [`day_verdict`]): the best kind
//!   there are enough hours in a row of.
//!
//! The app's core judges a take-off with these, and the server rates the
//! cells of its thermal map with them ([`cell_hours`], [`cell_days`]), so
//! that the map and a take-off's page say the same of the same air. They
//! differ in the height they judge for: a take-off's own, and for a cell
//! the model's ground, which lies below most take-offs in the mountains.
//!
//! The limits are our own and to verify against days that were flown. None
//! of this is advice to fly: it reads a weather model, which knows a cell
//! two kilometres wide and nothing of a slope.

use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use venti_domain::{ForecastHour, ForecastSeries, GeoPoint};

use crate::convection::{Convection, convection};
use crate::sun;

/// Rain or snow in an hour, millimetres, from which the hour is not for
/// flying.
pub const RAIN_MM: f64 = 0.2;
/// Mean wind 10 m above the model's ground, km/h: marginal from the
/// first, not flyable from the second.
pub const WIND_KMH: (f64, f64) = (20.0, 30.0);
/// The same for the gusts.
pub const GUSTS_KMH: (f64, f64) = (30.0, 40.0);
/// Wind higher up, at the model's levels within [`ALOFT_REACH_M`] above
/// the place, km/h: marginal from the first, not flyable from the second.
/// A calm valley cell under a strong wind at the ridges is the day this
/// is for.
pub const ALOFT_KMH: (f64, f64) = (35.0, 50.0);
/// How far above the place the wind higher up is looked at, metres.
pub const ALOFT_REACH_M: f64 = 1500.0;
/// A base of convective cloud less than this above the place is marginal,
/// metres; one below the place is not flyable.
pub const CLOUD_BASE_CLEAR_M: f64 = 200.0;
/// Convective available potential energy from which showers and
/// thunderstorms can grow, J/kg: marginal.
pub const CAPE_J_KG: f64 = 800.0;
/// A stretch of flyable hours counts from this many in a row.
pub const WINDOW_HOURS: usize = 2;
/// The climb a paraglider needs in thermals, m/s: to stay up around the
/// take-off, to fly cross-country, and on a big day.
pub const CLIMB_MS: [f64; 3] = [0.5, 1.0, 2.0];
/// How far above the place the thermals have to reach for the same three,
/// metres.
pub const REACH_M: [f64; 3] = [300.0, 800.0, 1500.0];
/// The strongest wind in the thermals' layer with which an hour still
/// counts for cross-country, and for a big day, km/h. More wind than this
/// tears thermals apart and carries a circling glider away.
pub const DRIFT_KMH: [f64; 2] = [25.0, 20.0];
/// How many hours in a row make a day of local thermals, a cross-country
/// day and a big day.
pub const THERMAL_HOURS: [usize; 3] = [2, 4, 6];

/// How an hour or a day looks for flying, by a forecast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    /// Nothing in the forecast speaks against it.
    Flyable,
    /// Something does.
    Marginal,
    /// The forecast rules it out.
    NotFlyable,
}

/// One thing a forecast has against an hour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Against {
    /// Rain or snow.
    Rain,
    /// The mean wind 10 m above the model's ground.
    Wind {
        /// Its speed, km/h.
        kmh: f64,
    },
    /// The gusts there.
    Gusts {
        /// Their speed, km/h.
        kmh: f64,
    },
    /// The strongest wind at the model's levels over the place.
    WindAloft {
        /// Its speed, km/h.
        kmh: f64,
        /// The height of the level, metres above mean sea level.
        at_m: f64,
    },
    /// Convective cloud with its base below the place.
    CloudBaseBelow,
    /// Convective cloud with its base just above the place.
    CloudBaseNear,
    /// Air in which showers or thunderstorms can grow.
    Storms,
}

/// What a day's hours mostly have against them: the kinds of [`Against`],
/// without their figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    /// Rain or snow.
    Rain,
    /// The mean wind at the ground.
    Wind,
    /// The gusts there.
    Gusts,
    /// The wind at the model's levels above the place.
    WindAloft,
    /// Convective cloud with its base at or just above the place.
    CloudBase,
    /// Air in which showers or thunderstorms can grow.
    Storms,
}

impl Against {
    /// The kind of thing this is, without its figure.
    #[must_use]
    pub fn reason(&self) -> Reason {
        match self {
            Self::Rain => Reason::Rain,
            Self::Wind { .. } => Reason::Wind,
            Self::Gusts { .. } => Reason::Gusts,
            Self::WindAloft { .. } => Reason::WindAloft,
            Self::CloudBaseBelow | Self::CloudBaseNear => Reason::CloudBase,
            Self::Storms => Reason::Storms,
        }
    }
}

/// What the forecast has against an hour by the air alone, each with how
/// much it counts, in a fixed order: the wind, the gusts, the wind higher
/// up, the cloud base, storms. Rain ends the hour and stands alone.
///
/// `place_m` is the height of the place the hour is judged for, metres
/// above mean sea level: a take-off, or the model's ground. Without it the
/// wind higher up and the cloud base are not weighed.
#[must_use]
pub fn against(hour: &ForecastHour, place_m: Option<f64>) -> Vec<(Verdict, Against)> {
    let kmh = |speed_ms: f64| speed_ms * 3.6;
    let mut found: Vec<(Verdict, Against)> = Vec::new();
    let mut weigh = |value: f64, limits: (f64, f64), what: Against| {
        if value >= limits.1 {
            found.push((Verdict::NotFlyable, what));
        } else if value >= limits.0 {
            found.push((Verdict::Marginal, what));
        }
    };

    if hour.precipitation_mm.is_some_and(|rain| rain >= RAIN_MM) {
        return vec![(Verdict::NotFlyable, Against::Rain)];
    }
    if let Some(speed) = hour.wind_speed_ms.map(kmh) {
        weigh(speed, WIND_KMH, Against::Wind { kmh: speed });
    }
    if let Some(gusts) = hour.wind_gust_ms.map(kmh) {
        weigh(gusts, GUSTS_KMH, Against::Gusts { kmh: gusts });
    }
    if let Some(place) = place_m {
        // The strongest wind at the model's levels over the place.
        let aloft = hour
            .winds_aloft
            .iter()
            .filter_map(|level| Some((level.altitude_m?, kmh(level.wind_speed_ms?))))
            .filter(|(altitude, _)| (place..=place + ALOFT_REACH_M).contains(altitude))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((altitude, speed)) = aloft {
            weigh(
                speed,
                ALOFT_KMH,
                Against::WindAloft {
                    kmh: speed,
                    at_m: altitude,
                },
            );
        }
        if let Some(base) = hour.convective_cloud_base_m {
            if base < place {
                found.push((Verdict::NotFlyable, Against::CloudBaseBelow));
            } else if base < place + CLOUD_BASE_CLEAR_M {
                found.push((Verdict::Marginal, Against::CloudBaseNear));
            }
        }
    }
    if hour.cape_j_kg.is_some_and(|cape| cape >= CAPE_J_KG) {
        found.push((Verdict::Marginal, Against::Storms));
    }
    found
}

/// The verdict a list of things against an hour comes to, with the worst
/// of them: of two equally bad the one listed first. Flyable, with
/// nothing, when the list is empty.
#[must_use]
pub fn worst<T: Clone>(found: &[(Verdict, T)]) -> (Verdict, Option<T>) {
    let verdict = found
        .iter()
        .map(|(verdict, _)| *verdict)
        .max()
        .unwrap_or(Verdict::Flyable);
    let what = found
        .iter()
        .find(|(of, _)| *of == verdict)
        .map(|(_, what)| what.clone());
    (verdict, what)
}

/// What thermals a forecast has: the kind of day they make.
///
/// Read from the model's heat and temperatures by the limits above. An
/// estimate of the air over a grid cell, never a promise of a climb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// None a paraglider climbs in, or none that reach above the place.
    Weak,
    /// Enough to stay up around the take-off.
    Local,
    /// Strong and high enough to leave with.
    CrossCountry,
    /// Strong, high and for most of the day.
    Big,
}

/// What an hour's thermals count as for a place at `place_m`, in an hour
/// with this verdict. None in an hour that is not flyable.
#[must_use]
pub fn kind_of_hour(air: &Convection, place_m: f64, verdict: Verdict) -> Option<Kind> {
    let reach = air.top_m - place_m;
    let calm = |limit_kmh: f64| air.wind_ms.is_none_or(|wind| wind * 3.6 <= limit_kmh);
    let by_the_air = if air.climb_ms >= CLIMB_MS[2] && reach >= REACH_M[2] && calm(DRIFT_KMH[1]) {
        Kind::Big
    } else if air.climb_ms >= CLIMB_MS[1] && reach >= REACH_M[1] && calm(DRIFT_KMH[0]) {
        Kind::CrossCountry
    } else if air.climb_ms >= CLIMB_MS[0] && reach >= REACH_M[0] {
        Kind::Local
    } else {
        Kind::Weak
    };
    match verdict {
        Verdict::Flyable => Some(by_the_air),
        // Something speaks against the hour: it is none to leave in.
        Verdict::Marginal => Some(by_the_air.min(Kind::Local)),
        Verdict::NotFlyable => None,
    }
}

/// An hour of a day as it was judged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RatedHour {
    /// When the hour begins.
    pub time: DateTime<Utc>,
    /// Whether it can be flown.
    pub verdict: Verdict,
    /// The worst thing the air has against it. None when nothing has, and
    /// where the caller keeps its own account of what speaks against the
    /// hour.
    pub against: Option<Against>,
    /// What the model makes of its thermals, where it gives what that
    /// takes.
    pub air: Option<Convection>,
    /// What they count as.
    pub kind: Option<Kind>,
}

/// The longest stretch of hours in a row that `counts`, as the indices of
/// its first and its last hour, if it is at least `needed` hours long. The
/// earliest of equally long ones. Hours in a row are an hour apart: a gap
/// in the model's hours ends a stretch.
#[must_use]
pub fn longest_run(
    hours: &[RatedHour],
    needed: usize,
    counts: impl Fn(&RatedHour) -> bool,
) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut start = None;
    for (index, hour) in hours.iter().enumerate() {
        let follows = index > 0 && hour.time - hours[index - 1].time == Duration::hours(1);
        if !counts(hour) {
            start = None;
            continue;
        }
        let first = match start {
            Some(first) if follows => first,
            _ => index,
        };
        start = Some(first);
        if best.is_none_or(|(from, to)| index - first > to - from) {
            best = Some((first, index));
        }
    }
    best.filter(|(from, to)| to - from + 1 >= needed)
}

/// The longest stretch of hours in a row that are no worse than `limit`,
/// if it is at least [`WINDOW_HOURS`] long.
#[must_use]
pub fn stretch(hours: &[RatedHour], limit: Verdict) -> Option<(usize, usize)> {
    longest_run(hours, WINDOW_HOURS, |hour| hour.verdict <= limit)
}

/// The verdict on a day's hours: flyable with a stretch of flyable hours,
/// marginal with a stretch of hours that are at worst marginal, else not
/// flyable.
#[must_use]
pub fn day_verdict(hours: &[RatedHour]) -> Verdict {
    if stretch(hours, Verdict::Flyable).is_some() {
        Verdict::Flyable
    } else if stretch(hours, Verdict::Marginal).is_some() {
        Verdict::Marginal
    } else {
        Verdict::NotFlyable
    }
}

/// What a day's thermals come to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermalDay {
    /// The kind of day.
    pub kind: Kind,
    /// The hours that make it, as the indices of the first and the last;
    /// none on a day of weak thermals.
    pub window: Option<(usize, usize)>,
    /// The climb to expect, m/s: the mean over the window, or the best
    /// hour's on a day of weak thermals. Not rounded.
    pub climb_ms: f64,
    /// How high the thermals go, metres above mean sea level: the highest
    /// of the window, or the best hour's on a day of weak thermals. None
    /// without thermals. Not rounded.
    pub top_m: Option<f64>,
    /// Whether that height is a base of cumulus.
    pub cloud: bool,
}

/// The kind of day the hours' thermals make. `None` when no hour that can
/// be flown has an estimate. For a day that can be flown: a day that is
/// not flyable says nothing of thermals, which is the caller's to hold to.
#[must_use]
pub fn thermal_day(hours: &[RatedHour]) -> Option<ThermalDay> {
    // The best kind there are enough hours in a row of.
    let found = [Kind::Big, Kind::CrossCountry, Kind::Local]
        .into_iter()
        .zip([THERMAL_HOURS[2], THERMAL_HOURS[1], THERMAL_HOURS[0]])
        .find_map(|(kind, needed)| {
            longest_run(hours, needed, |hour| hour.kind >= Some(kind)).map(|run| (kind, run))
        });
    if let Some((kind, (from, to))) = found {
        let airs: Vec<Convection> = hours[from..=to]
            .iter()
            .filter_map(|hour| hour.air)
            .collect();
        let climb_ms = airs.iter().map(|air| air.climb_ms).sum::<f64>() / airs.len() as f64;
        let highest = airs
            .iter()
            .max_by(|a, b| a.top_m.total_cmp(&b.top_m))
            .copied()?;
        return Some(ThermalDay {
            kind,
            window: Some((from, to)),
            climb_ms,
            top_m: Some(highest.top_m),
            cloud: highest.cloud_base_m.is_some(),
        });
    }
    // Weak: the best there is, among the hours that can be flown.
    let best = hours
        .iter()
        .filter(|hour| hour.kind.is_some())
        .filter_map(|hour| hour.air)
        .max_by(|a, b| a.climb_ms.total_cmp(&b.climb_ms))?;
    Some(ThermalDay {
        kind: Kind::Weak,
        window: None,
        climb_ms: best.climb_ms,
        top_m: (best.depth_m > 0.0).then_some(best.top_m),
        cloud: best.depth_m > 0.0 && best.cloud_base_m.is_some(),
    })
}

/// The day by the sun that an hour belongs to at a place: the sun's own
/// clock runs four minutes a degree ahead of Greenwich's.
fn solar_date(time: DateTime<Utc>, place: GeoPoint) -> NaiveDate {
    (time + Duration::seconds((place.lon() * 240.0).round() as i64)).date_naive()
}

/// The hours of the clock that count as a day's daylight at a place: from
/// an hour after sunrise to an hour before sunset, each given by its
/// beginning. The first and the last hour of the sun are not for a
/// judgement by wind and cloud alone. Empty where the sun does not rise
/// or does not set that day.
#[must_use]
pub fn daylight_hours(date: NaiveDate, place: GeoPoint) -> Vec<DateTime<Utc>> {
    let light = sun::daylight(date, place);
    let (Some(rise), Some(set)) = (light.sunrise, light.sunset) else {
        return Vec::new();
    };
    // The first whole hour that begins an hour or more after sunrise.
    let earliest = rise + Duration::hours(1);
    let mut hour = earliest
        .with_minute(0)
        .and_then(|time| time.with_second(0))
        .and_then(|time| time.with_nanosecond(0))
        .unwrap_or(earliest);
    if hour < earliest {
        hour += Duration::hours(1);
    }
    let mut hours = Vec::new();
    while hour + Duration::hours(1) <= set {
        hours.push(hour);
        hour += Duration::hours(1);
    }
    hours
}

/// The daylight hours of a model's series, each judged for the model's own
/// cell: by the air alone, at the height of the model's ground there.
/// Oldest first. Where the series does not say how high its ground is, the
/// wind higher up and the cloud base are not weighed and no hour has
/// thermals.
#[must_use]
pub fn cell_hours(series: &ForecastSeries) -> Vec<RatedHour> {
    let ground = series.cell_elevation_m;
    let mut light: Option<(NaiveDate, Vec<DateTime<Utc>>)> = None;
    series
        .hours
        .iter()
        .filter(|hour| {
            let date = solar_date(hour.time, series.cell);
            if light.as_ref().is_none_or(|(of, _)| *of != date) {
                light = Some((date, daylight_hours(date, series.cell)));
            }
            light
                .as_ref()
                .is_some_and(|(_, hours)| hours.contains(&hour.time))
        })
        .map(|hour| {
            let (verdict, found) = worst(&against(hour, ground));
            let air = ground.and_then(|ground| convection(hour, ground));
            let kind = air
                .zip(ground)
                .and_then(|(air, ground)| kind_of_hour(&air, ground, verdict));
            RatedHour {
                time: hour.time,
                verdict,
                against: found,
                air,
                kind,
            }
        })
        .collect()
}

/// A day of a cell of the thermal map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellDay {
    /// The day, by the sun at the cell.
    pub date: NaiveDate,
    /// Whether every daylight hour of the day was there to be judged. A
    /// day that lacks hours is judged by the ones it has, which can make
    /// it look worse than it is: six hours in a row cannot be found among
    /// four.
    pub whole: bool,
    /// Whether the day can be flown, by the air alone.
    pub verdict: Verdict,
    /// What speaks against the day: the thing met most often among the
    /// hours that are no better than the day's verdict, and of equally
    /// frequent ones the earliest. None on a flyable day.
    pub why: Option<Reason>,
    /// What its thermals come to. None on a day that is not flyable, and
    /// where the model does not give what an estimate takes.
    pub thermals: Option<ThermalDay>,
}

/// The days that a cell's judged hours make, oldest first. `hours` are
/// daylight hours as [`cell_hours`] gives them, oldest first, and may come
/// from more than one run of a model; `cell` is the cell's centre.
#[must_use]
pub fn cell_days(hours: &[RatedHour], cell: GeoPoint) -> Vec<CellDay> {
    let mut days = Vec::new();
    let mut from = 0;
    while from < hours.len() {
        let date = solar_date(hours[from].time, cell);
        let to = hours[from..]
            .iter()
            .position(|hour| solar_date(hour.time, cell) != date)
            .map_or(hours.len(), |next| from + next);
        let of_day = &hours[from..to];
        let verdict = day_verdict(of_day);
        days.push(CellDay {
            date,
            why: most_met(of_day, verdict),
            whole: daylight_hours(date, cell)
                .iter()
                .all(|needed| of_day.iter().any(|hour| hour.time == *needed)),
            verdict,
            thermals: (verdict != Verdict::NotFlyable)
                .then(|| thermal_day(of_day))
                .flatten(),
        });
        from = to;
    }
    days
}

/// What the hours of a day with this verdict mostly have against them.
fn most_met(hours: &[RatedHour], verdict: Verdict) -> Option<Reason> {
    if verdict == Verdict::Flyable {
        return None;
    }
    let mut counts: Vec<(Reason, usize)> = Vec::new();
    for reason in hours
        .iter()
        .filter(|hour| hour.verdict >= verdict)
        .filter_map(|hour| hour.against.map(|against| against.reason()))
    {
        match counts.iter_mut().find(|(of, _)| *of == reason) {
            Some((_, count)) => *count += 1,
            None => counts.push((reason, 1)),
        }
    }
    let most = counts.iter().map(|(_, count)| *count).max()?;
    counts
        .into_iter()
        .find(|(_, count)| *count == most)
        .map(|(reason, _)| reason)
}

/// A height to the nearest fifty metres: the estimate is no finer.
#[must_use]
pub fn fifty(metres: f64) -> f64 {
    (metres / 50.0).round() * 50.0
}

/// A climb to a tenth of a metre per second.
#[must_use]
pub fn tenth(ms: f64) -> f64 {
    (ms * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use venti_domain::{ModelId, SourceId, WindAloft};

    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 12, hour, 0, 0)
            .single()
            .unwrap_or_default()
    }

    fn air(climb_ms: f64, top_m: f64, wind_ms: f64) -> Convection {
        Convection {
            top_m,
            cloud_base_m: None,
            depth_m: top_m - 1000.0,
            updraft_ms: climb_ms + 1.0,
            climb_ms,
            wind_ms: Some(wind_ms),
        }
    }

    fn rated(hour: u32, verdict: Verdict, air: Convection, place_m: f64) -> RatedHour {
        RatedHour {
            time: at(hour),
            verdict,
            against: None,
            air: Some(air),
            kind: kind_of_hour(&air, place_m, verdict),
        }
    }

    #[test]
    fn rain_ends_an_hour_and_stands_alone() {
        let mut hour = ForecastHour::empty(at(12));
        hour.precipitation_mm = Some(0.4);
        hour.wind_speed_ms = Some(12.0);
        assert_eq!(
            against(&hour, Some(1000.0)),
            vec![(Verdict::NotFlyable, Against::Rain)]
        );
    }

    #[test]
    fn wind_and_gusts_count_from_their_limits() {
        let mut hour = ForecastHour::empty(at(12));
        hour.wind_speed_ms = Some(6.0);
        hour.wind_gust_ms = Some(12.0);
        let found = against(&hour, None);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, Verdict::Marginal);
        assert!(matches!(found[0].1, Against::Wind { kmh } if (kmh - 21.6).abs() < 1e-9));
        assert_eq!(found[1].0, Verdict::NotFlyable);
        assert!(matches!(found[1].1, Against::Gusts { kmh } if (kmh - 43.2).abs() < 1e-9));
        let (verdict, what) = worst(&found);
        assert_eq!(verdict, Verdict::NotFlyable);
        assert!(matches!(what, Some(Against::Gusts { .. })));
    }

    #[test]
    fn the_wind_higher_up_and_the_cloud_base_need_the_height_of_the_place() {
        let mut hour = ForecastHour::empty(at(12));
        let level = |hpa: f64, altitude_m: f64, wind_ms: f64| {
            let mut level = WindAloft::at_pressure(hpa);
            level.altitude_m = Some(altitude_m);
            level.wind_speed_ms = Some(wind_ms);
            level
        };
        // The second is too far above the place to count.
        hour.winds_aloft = vec![level(850.0, 1500.0, 11.0), level(700.0, 3000.0, 30.0)];
        hour.convective_cloud_base_m = Some(1100.0);
        assert!(against(&hour, None).is_empty());
        let found = against(&hour, Some(1000.0));
        assert_eq!(found.len(), 2);
        assert!(
            matches!(found[0], (Verdict::Marginal, Against::WindAloft { at_m, .. }) if (at_m - 1500.0).abs() < 1e-9)
        );
        assert_eq!(found[1], (Verdict::Marginal, Against::CloudBaseNear));
        assert_eq!(
            against(&hour, Some(1200.0)).last().copied(),
            Some((Verdict::NotFlyable, Against::CloudBaseBelow))
        );
    }

    #[test]
    fn an_hour_with_nothing_against_it_is_flyable() {
        let hour = ForecastHour::empty(at(12));
        assert_eq!(
            worst(&against(&hour, Some(1000.0))),
            (Verdict::Flyable, None)
        );
    }

    #[test]
    fn an_hours_thermals_count_by_climb_reach_and_wind() {
        let place = 1000.0;
        let kind = |air: Convection| kind_of_hour(&air, place, Verdict::Flyable);
        assert_eq!(kind(air(2.2, 2600.0, 4.0)), Some(Kind::Big));
        // Too much wind for a big day, little enough for cross-country.
        assert_eq!(kind(air(2.2, 2600.0, 6.5)), Some(Kind::CrossCountry));
        // Strong and windy: thermals to stay up in, none to leave with.
        assert_eq!(kind(air(2.2, 2600.0, 8.0)), Some(Kind::Local));
        // Strong, and not high enough for a big day.
        assert_eq!(kind(air(2.2, 2400.0, 2.0)), Some(Kind::CrossCountry));
        assert_eq!(kind(air(0.6, 1400.0, 2.0)), Some(Kind::Local));
        assert_eq!(kind(air(0.6, 1200.0, 2.0)), Some(Kind::Weak));
        assert_eq!(kind(air(0.3, 2600.0, 2.0)), Some(Kind::Weak));
    }

    #[test]
    fn a_marginal_hour_is_none_to_leave_in_and_one_not_flyable_has_no_kind() {
        let strong = air(2.5, 3000.0, 2.0);
        assert_eq!(
            kind_of_hour(&strong, 1000.0, Verdict::Marginal),
            Some(Kind::Local)
        );
        assert_eq!(kind_of_hour(&strong, 1000.0, Verdict::NotFlyable), None);
    }

    #[test]
    fn a_big_day_takes_six_strong_hours_in_a_row() {
        let strong = air(2.4, 3000.0, 2.0);
        let six: Vec<RatedHour> = (10..16)
            .map(|hour| rated(hour, Verdict::Flyable, strong, 1000.0))
            .collect();
        let day = thermal_day(&six).unwrap_or_else(|| unreachable!());
        assert_eq!(day.kind, Kind::Big);
        assert_eq!(day.window, Some((0, 5)));
        assert!((day.climb_ms - 2.4).abs() < 1e-9);
        assert_eq!(day.top_m, Some(3000.0));
        assert!(!day.cloud);

        // Five of them are a cross-country day.
        let day = thermal_day(&six[..5]).unwrap_or_else(|| unreachable!());
        assert_eq!(day.kind, Kind::CrossCountry);
        assert_eq!(day.window, Some((0, 4)));
    }

    #[test]
    fn a_gap_in_the_hours_ends_a_stretch() {
        let strong = air(2.4, 3000.0, 2.0);
        let hours: Vec<RatedHour> = [9, 10, 11, 13, 14, 15]
            .into_iter()
            .map(|hour| rated(hour, Verdict::Flyable, strong, 1000.0))
            .collect();
        // Three and three, never six: no big day, and no four for
        // cross-country either.
        let day = thermal_day(&hours).unwrap_or_else(|| unreachable!());
        assert_eq!(day.kind, Kind::Local);
        assert_eq!(day.window, Some((0, 2)));
    }

    #[test]
    fn a_day_without_a_stretch_is_weak_and_says_the_best_there_is() {
        let hours = vec![
            rated(11, Verdict::Flyable, air(0.2, 1500.0, 2.0), 1000.0),
            rated(12, Verdict::Flyable, air(0.4, 1800.0, 2.0), 1000.0),
            rated(13, Verdict::NotFlyable, air(3.0, 3000.0, 2.0), 1000.0),
        ];
        let day = thermal_day(&hours).unwrap_or_else(|| unreachable!());
        assert_eq!(day.kind, Kind::Weak);
        assert_eq!(day.window, None);
        // The hour that cannot be flown does not speak for the day.
        assert!((day.climb_ms - 0.4).abs() < 1e-9);
        assert_eq!(day.top_m, Some(1800.0));
    }

    #[test]
    fn a_day_with_no_hour_to_fly_has_no_thermals_to_name() {
        let hours = vec![rated(
            12,
            Verdict::NotFlyable,
            air(3.0, 3000.0, 2.0),
            1000.0,
        )];
        assert_eq!(thermal_day(&hours), None);
        assert_eq!(day_verdict(&hours), Verdict::NotFlyable);
    }

    #[test]
    fn a_days_verdict_takes_two_hours_in_a_row() {
        let calm = air(1.0, 2000.0, 2.0);
        let hour = |hour, verdict| rated(hour, verdict, calm, 1000.0);
        assert_eq!(
            day_verdict(&[
                hour(10, Verdict::Flyable),
                hour(11, Verdict::Marginal),
                hour(12, Verdict::Flyable),
            ]),
            Verdict::Marginal
        );
        assert_eq!(
            day_verdict(&[hour(10, Verdict::Flyable), hour(11, Verdict::Flyable)]),
            Verdict::Flyable
        );
        assert_eq!(
            day_verdict(&[hour(10, Verdict::Flyable), hour(11, Verdict::NotFlyable)]),
            Verdict::NotFlyable
        );
    }

    /// A place in the Bavarian Alps: the sun stands highest there some
    /// 47 minutes before noon by Greenwich's clock.
    fn cell() -> GeoPoint {
        GeoPoint::new(47.7, 11.8).unwrap_or_else(|_| unreachable!())
    }

    /// A summer hour over a cell whose ground lies at 1000 m, with the
    /// heat the ground gives the air in it. With 450 W/m² the thermals
    /// reach a cloud base at 3250 m and a glider climbs at 2.1 m/s.
    fn summer(time: DateTime<Utc>, heat_w_m2: f64) -> ForecastHour {
        let level = |hpa: f64, altitude_m: f64, temperature_c: f64, wind_ms: f64| {
            let mut level = WindAloft::at_pressure(hpa);
            level.altitude_m = Some(altitude_m);
            level.temperature_c = Some(temperature_c);
            level.wind_speed_ms = Some(wind_ms);
            level
        };
        let mut hour = ForecastHour::empty(time);
        hour.temperature_c = Some(26.0);
        hour.dew_point_c = Some(8.0);
        hour.heat_to_air_w_m2 = Some(heat_w_m2);
        hour.wind_speed_ms = Some(2.0);
        hour.winds_aloft = vec![
            level(850.0, 1500.0, 19.0, 3.0),
            level(700.0, 3100.0, 4.5, 4.0),
            level(600.0, 4300.0, -3.0, 8.0),
        ];
        hour
    }

    fn series(hours: Vec<ForecastHour>) -> ForecastSeries {
        ForecastSeries {
            source: SourceId::new("open-meteo").unwrap_or_else(|_| unreachable!()),
            model: ModelId::new("icon-d2").unwrap_or_else(|_| unreachable!()),
            run: at(0),
            available_at: None,
            point: cell(),
            cell: cell(),
            cell_elevation_m: Some(1000.0),
            hours,
        }
    }

    #[test]
    fn daylight_is_from_an_hour_after_sunrise_to_an_hour_before_sunset() {
        let date = at(12).date_naive();
        let light = sun::daylight(date, cell());
        let (rise, set) = (
            light.sunrise.unwrap_or_default(),
            light.sunset.unwrap_or_default(),
        );
        let hours = daylight_hours(date, cell());
        let (first, last) = (hours[0], hours[hours.len() - 1]);
        assert!(first >= rise + Duration::hours(1));
        assert!(first - Duration::hours(1) < rise + Duration::hours(1));
        assert!(last + Duration::hours(1) <= set);
        assert!(last + Duration::hours(2) > set);
        // In the middle of July in the Alps the sun is up for more than
        // fifteen hours: fourteen of them count.
        assert_eq!(hours.len(), 14);
    }

    #[test]
    fn a_cells_hours_are_its_daylight_hours_judged_at_the_models_ground() {
        let hours: Vec<ForecastHour> = (0..24).map(|hour| summer(at(hour), 450.0)).collect();
        let rated = cell_hours(&series(hours));
        let light = daylight_hours(at(12).date_naive(), cell());
        assert_eq!(
            rated.iter().map(|hour| hour.time).collect::<Vec<_>>(),
            light
        );
        assert!(rated.iter().all(|hour| hour.verdict == Verdict::Flyable));
        assert!(rated.iter().all(|hour| hour.kind == Some(Kind::Big)));
    }

    #[test]
    fn a_cell_without_the_height_of_its_ground_has_a_verdict_and_no_thermals() {
        let mut series = series(vec![summer(at(11), 300.0), summer(at(12), 300.0)]);
        series.cell_elevation_m = None;
        let rated = cell_hours(&series);
        assert_eq!(rated.len(), 2);
        assert!(rated.iter().all(|hour| hour.verdict == Verdict::Flyable));
        assert!(
            rated
                .iter()
                .all(|hour| hour.air.is_none() && hour.kind.is_none())
        );
        let days = cell_days(&rated, cell());
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].verdict, Verdict::Flyable);
        assert_eq!(days[0].thermals, None);
        assert!(!days[0].whole);
    }

    #[test]
    fn a_cells_days_are_days_by_the_sun_and_say_whether_they_are_whole() {
        // All of the twelfth, and of the thirteenth the hours from ten on,
        // as a model's later run has them.
        let mut hours: Vec<ForecastHour> = (0..24).map(|hour| summer(at(hour), 450.0)).collect();
        hours.extend((10..24).map(|hour| summer(at(hour) + Duration::days(1), 450.0)));
        let days = cell_days(&cell_hours(&series(hours)), cell());
        assert_eq!(days.len(), 2);

        assert_eq!(days[0].date, at(12).date_naive());
        assert!(days[0].whole);
        assert_eq!(days[0].verdict, Verdict::Flyable);
        let thermals = days[0].thermals.unwrap_or_else(|| unreachable!());
        assert_eq!(thermals.kind, Kind::Big);
        assert_eq!(thermals.window, Some((0, 13)));

        assert_eq!(days[1].date, at(12).date_naive() + Duration::days(1));
        assert!(!days[1].whole);
        // Enough hours are left of it for a big day all the same.
        assert_eq!(days[1].thermals.map(|day| day.kind), Some(Kind::Big));
    }

    #[test]
    fn a_rained_out_day_of_a_cell_is_not_flyable_and_names_no_thermals() {
        let hours: Vec<ForecastHour> = (0..24)
            .map(|hour| {
                let mut hour = summer(at(hour), 450.0);
                hour.precipitation_mm = Some(1.2);
                hour
            })
            .collect();
        let days = cell_days(&cell_hours(&series(hours)), cell());
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].verdict, Verdict::NotFlyable);
        assert_eq!(days[0].why, Some(Reason::Rain));
        assert_eq!(days[0].thermals, None);
        assert!(days[0].whole);
    }

    #[test]
    fn a_day_says_what_its_hours_mostly_have_against_them() {
        // Gusts in the morning, strong wind higher up from eleven on: a
        // marginal day, and mostly by the wind higher up.
        let hours: Vec<ForecastHour> = (0..24)
            .map(|hour| {
                let mut forecast = summer(at(hour), 450.0);
                if hour < 11 {
                    forecast.wind_gust_ms = Some(9.0);
                } else {
                    forecast.winds_aloft[0].wind_speed_ms = Some(11.0);
                }
                forecast
            })
            .collect();
        let days = cell_days(&cell_hours(&series(hours)), cell());
        assert_eq!(days[0].verdict, Verdict::Marginal);
        assert_eq!(days[0].why, Some(Reason::WindAloft));

        // A day with nothing against it names nothing.
        let fine: Vec<ForecastHour> = (0..24).map(|hour| summer(at(hour), 450.0)).collect();
        let days = cell_days(&cell_hours(&series(fine)), cell());
        assert_eq!(days[0].verdict, Verdict::Flyable);
        assert_eq!(days[0].why, None);
    }

    #[test]
    fn heights_round_to_fifty_metres_and_climbs_to_a_tenth() {
        assert!((fifty(2474.0) - 2450.0).abs() < 1e-9);
        assert!((fifty(2475.0) - 2500.0).abs() < 1e-9);
        assert!((tenth(1.84) - 1.8).abs() < 1e-9);
    }
}
