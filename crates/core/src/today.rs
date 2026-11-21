//! Today: what a forecast says about flying from a take-off, hour by hour.
//!
//! The shell asks the server for the forecast of the take-off's place
//! ([`crate::forecast_path`]) and hands the answer here with what the site's
//! source says about it: the wind directions it works in and its height.
//! Out comes a reading of the finest model: each daylight hour as flyable,
//! marginal or not flyable with the reason in words, and each day in a
//! line, "Good from noon". Beside it stands what the model's heat and
//! temperatures make of the thermals ([`venti_flight::convection`]): the
//! climb to expect and how high it goes for each hour, and what kind of
//! day that makes, from weak thermals to a big day.
//!
//! It is a reading of a weather model, not a judgement of the take-off. The
//! model speaks for a grid cell kilometres wide, whose ground can lie
//! hundreds of metres from the take-off's; it knows no rotor, no valley
//! wind and nothing of the pilot. So the words never say "safe" or "go",
//! every limit below is plain to read, and the view names the model, its
//! run and its cell. The limits are our own, round numbers a cautious
//! pilot would nod to. To verify with pilots, all of them.

use chrono::{DateTime, Duration, Timelike, Utc};
use venti_api_types::v1::ForecastResponse;
use venti_domain::{ForecastHour, ForecastSeries};
use venti_flight::convection::{Convection, convection};
use venti_flight::sun;

use crate::airspace::compass;
use crate::forecast::wind as forecast_wind;
use crate::server::answer;
use crate::sites::{WindSpan, wind_fits, wind_spans_in_words};
use crate::wind::credit;
use crate::{CoreError, Credit, ForecastWind};

/// Rain or snow in an hour, millimetres, from which the hour is not for
/// flying.
const RAIN_MM: f64 = 0.2;
/// Mean wind 10 m above the model's ground, km/h: marginal from the
/// first, not flyable from the second.
const WIND_KMH: (f64, f64) = (20.0, 30.0);
/// The same for the gusts.
const GUSTS_KMH: (f64, f64) = (30.0, 40.0);
/// A wind that does not come from a direction the take-off works in:
/// below the first it is light enough not to count, from the second it
/// makes the hour not flyable, in between marginal. km/h.
const WRONG_WAY_KMH: (f64, f64) = (8.0, 15.0);
/// Wind higher up, at the model's levels within [`ALOFT_REACH_M`] above
/// the take-off, km/h: marginal from the first, not flyable from the
/// second. A calm valley cell under a strong wind at the ridges is the
/// day this is for.
const ALOFT_KMH: (f64, f64) = (35.0, 50.0);
/// How far above the take-off the wind higher up is looked at, metres.
const ALOFT_REACH_M: f64 = 1500.0;
/// A base of convective cloud less than this above the take-off is
/// marginal, metres; one below the take-off is not flyable.
const CLOUD_BASE_CLEAR_M: f64 = 200.0;
/// Convective available potential energy from which showers and
/// thunderstorms can grow, J/kg: marginal.
const CAPE_J_KG: f64 = 800.0;
/// A stretch of flyable hours counts from this many in a row.
const WINDOW_HOURS: usize = 2;
/// Up to this height above sea level an hour lists the wind at the
/// model's levels, metres: the air a paraglider flies in.
const WINDS_UP_TO_M: f64 = 5000.0;
/// The climb a paraglider needs in thermals, m/s: to stay up around the
/// take-off, to fly cross-country, and on a big day.
const CLIMB_MS: [f64; 3] = [0.5, 1.0, 2.0];
/// How far above the take-off the thermals have to reach for the same
/// three, metres.
const REACH_M: [f64; 3] = [300.0, 800.0, 1500.0];
/// The strongest wind in the thermals' layer with which an hour still
/// counts for cross-country, and for a big day, km/h. More wind than this
/// tears thermals apart and carries a circling glider away.
const DRIFT_KMH: [f64; 2] = [25.0, 20.0];
/// How many hours in a row make a day of local thermals, a cross-country
/// day and a big day.
const THERMAL_HOURS: [usize; 3] = [2, 4, 6];

/// How a take-off looks for flying, by a forecast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum Flyability {
    /// Nothing in the forecast speaks against it.
    Flyable,
    /// Something does: read the reason.
    Marginal,
    /// The forecast rules it out.
    NotFlyable,
}

impl Flyability {
    /// The verdict as the design language words it. Never "safe", never
    /// "go".
    fn words(self) -> &'static str {
        match self {
            Self::Flyable => "Flyable",
            Self::Marginal => "Marginal",
            Self::NotFlyable => "Not flyable",
        }
    }
}

/// What thermals a forecast has for a take-off: the kind of day it makes.
///
/// Read from the model's heat and temperatures by the limits above, which
/// are our own. An estimate of the air over a grid cell, never a promise
/// of a climb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum Thermals {
    /// None a paraglider climbs in, or none that reach above the take-off.
    Weak,
    /// Enough to stay up around the take-off.
    Local,
    /// Strong and high enough to leave with.
    CrossCountry,
    /// Strong, high and for most of the day.
    Big,
}

impl Thermals {
    /// The kind of day in words, calm ones.
    fn words(self) -> &'static str {
        match self {
            Self::Weak => "Weak thermals",
            Self::Local => "Local thermals",
            Self::CrossCountry => "Cross-country day",
            Self::Big => "Big day",
        }
    }
}

/// The wind at one of the model's levels in an hour, for a picture of the
/// air over a take-off.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OutlookWind {
    /// How high the level lies, metres above sea level. For a level at a
    /// fixed height above the model's ground it is that ground's height
    /// with the level's added: the model's, not the terrain's.
    pub altitude_m: f64,
    /// The wind there.
    pub wind: ForecastWind,
}

/// One daylight hour at a take-off.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OutlookHour {
    /// The hour's first moment, seconds since 1970-01-01 UTC.
    pub time_unix_s: i64,
    /// The hour on the place's clock: "14".
    pub hour: String,
    /// What the forecast says about flying in it.
    pub verdict: Flyability,
    /// What speaks against it, in words: "Rain", "Gusts to 31 km/h".
    /// Empty for a flyable hour.
    pub why: String,
    /// Where the wind 10 m above the model's ground comes from, degrees
    /// clockwise from north.
    pub wind_from_deg: Option<f64>,
    /// Its speed in whole km/h: "12". Empty when the model gives none.
    pub wind_kmh: String,
    /// What the hour's thermals count as. A marginal hour counts as local
    /// thermals at most. `None` for an hour that is not flyable, and when
    /// the model does not give what the estimate is made from.
    pub thermals: Option<Thermals>,
    /// The climb to expect of a paraglider in the hour's thermals, m/s to
    /// a tenth; zero without thermals. `None` when the model does not give
    /// what the estimate is made from.
    pub climb_ms: Option<f64>,
    /// How high the thermals go, metres above sea level to the nearest
    /// fifty: the cloud base, or where they end in the blue. `None`
    /// without thermals.
    pub thermal_top_m: Option<f64>,
    /// Whether cumulus caps the thermals: `thermal_top_m` is then the
    /// cloud base. False on a blue day and without thermals.
    pub thermal_cloud: bool,
    /// The wind at the model's levels over its cell, lowest first, from
    /// 10 m above its ground to 5000 m above the sea. Empty where the
    /// model's ground is not known: a level's height cannot be said then.
    pub winds: Vec<OutlookWind>,
}

/// One day at a take-off.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OutlookDay {
    /// The day's first moment on the place's clock, seconds since
    /// 1970-01-01 UTC.
    pub start_unix_s: i64,
    /// What the day is called on the place's calendar: "Today",
    /// "Tomorrow", then the day of the week, "Fri".
    pub name: String,
    /// The day's verdict: flyable when at least two hours in a row are,
    /// marginal when two in a row are at least that.
    pub verdict: Flyability,
    /// The verdict in words: "Flyable", "Marginal", "Not flyable".
    pub status: String,
    /// The day in a line: "Good from noon", "Good from 11:00 to 15:00",
    /// "Marginal", "Not flyable".
    pub headline: String,
    /// What limits the day, in words: "Gusts to 31 km/h after 15:00",
    /// "Rain". Empty when nothing does.
    pub why: String,
    /// Its daylight hours still to come, the earliest first.
    pub hours: Vec<OutlookHour>,
    /// The wind at the middle of the day's best stretch, 10 m above the
    /// model's ground: its speed in whole km/h, "12". Empty without one.
    pub wind_kmh: String,
    /// Its gusts in whole km/h: "25". Empty when the model gives none.
    pub gust_kmh: String,
    /// Where that wind comes from, as a compass point: "N". Empty when
    /// the model gives none.
    pub wind_from: String,
    /// The base of convective cloud then, metres above sea level, where
    /// the model has such cloud.
    pub cloud_base_m: Option<f64>,
    /// The kind of day the thermals make: a big day when they are strong
    /// and high for six hours in a row, a cross-country day for four,
    /// local thermals for two. `None` on a day that is not flyable, and
    /// when the model does not give what the estimate is made from.
    pub thermals: Option<Thermals>,
    /// The same in words: "Cross-country day". Empty without it.
    pub thermal_words: String,
    /// The thermals in a line: "1.8 m/s to a cloud base of 2900 m, from
    /// noon to 17:00", "1.2 m/s to 2400 m in the blue, from 13:00 to
    /// 16:00", "0.3 m/s to 1500 m at best", "None to climb in". Empty
    /// without an estimate.
    pub thermal_line: String,
    /// The climb to expect over the day's thermal hours, m/s to a tenth:
    /// their mean. On a day of weak thermals, the best hour's.
    pub climb_ms: Option<f64>,
    /// How high the thermals go in those hours, metres above sea level to
    /// the nearest fifty: the highest of them.
    pub thermal_top_m: Option<f64>,
    /// How high the model takes the ground to be in its cell, metres above
    /// sea level: where the thermals of the hours begin, which in the
    /// mountains is not where the take-off stands.
    pub ground_m: Option<f64>,
    /// The model the day is read from: "ICON-D2". The finest the server
    /// has, unless a coarser one has more of the day's hours: the day a
    /// fine model's run ends in, and the days after it.
    pub model: String,
    /// When that model's run began, seconds since 1970-01-01 UTC.
    pub run_unix_s: i64,
    /// What the reading of that model is and is not, for the foot of the
    /// screen while this day is shown.
    pub note: String,
}

/// What a forecast says about flying from a take-off.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteOutlook {
    /// The days the model has daylight hours for, the earliest first, and
    /// today also when its hours are gone. A day that is over is not among
    /// them, and the first is not today where the forecast begins later:
    /// each day says what it is called. Empty when the server has no
    /// forecast for the place, or none that reaches into daylight.
    pub days: Vec<OutlookDay>,
    /// The finest model read: "ICON-D2". Empty without one. A day can be
    /// read from another: each says which.
    pub model: String,
    /// When that model's run began, seconds since 1970-01-01 UTC.
    pub run_unix_s: i64,
    /// When the server last made sure it had the newest runs.
    pub fetched_unix_s: i64,
    /// What the reading is and is not, for the foot of the screen.
    pub note: String,
    /// Whose data it is. The licences ask for this next to the data.
    pub credits: Vec<Credit>,
}

/// Reads the server's answer to [`crate::forecast_path`] for a take-off.
///
/// `wind` is where the take-off works, as its source says, and may be
/// empty; `elevation_m` its height, if known. `utc_offset_s` is how far the
/// clock at the place is ahead of UTC. Hours before `now_unix_s` are left
/// out: the day in a line is about what is still to come.
#[uniffi::export]
pub fn site_outlook(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
    wind: Vec<WindSpan>,
    elevation_m: Option<f64>,
    now_unix_s: i64,
) -> Result<SiteOutlook, CoreError> {
    let response: ForecastResponse = answer(status, &body)?;
    // The models that have anything to say, the finest first.
    let mut models: Vec<&ForecastSeries> = response
        .layers
        .iter()
        .flat_map(|layer| &layer.series)
        .filter(|series| !series.hours.is_empty())
        .collect();
    models.sort_by_key(|series| crate::forecast::model_rank(series.model.as_str()));
    let Some(series) = models.first().copied() else {
        return Ok(SiteOutlook {
            days: Vec::new(),
            model: String::new(),
            run_unix_s: 0,
            fetched_unix_s: response.fetched_at.timestamp(),
            note: String::new(),
            credits: response.attributions.iter().map(credit).collect(),
        });
    };
    let site = Site {
        wind: &wind,
        works_in: wind_spans_in_words(&wind),
        elevation_m,
    };
    // A day is one model's, never a blend of two: the finest model's,
    // unless a coarser one has more of the day's hours. That is the day a
    // fine model's run ends in, and the days after it.
    let mut chosen: Vec<OutlookDay> = Vec::new();
    for model in &models {
        for day in days(model, &site, utc_offset_s, now_unix_s) {
            match chosen
                .iter_mut()
                .find(|have| have.start_unix_s == day.start_unix_s)
            {
                Some(have) if day.hours.len() > have.hours.len() => *have = day,
                Some(_) => {}
                None => chosen.push(day),
            }
        }
    }
    chosen.sort_by_key(|day| day.start_unix_s);
    Ok(SiteOutlook {
        // What the finest model's reading is, also on a day with nothing
        // to say about thermals; each day carries its own.
        note: note(series, chosen.iter().any(|day| day.thermals.is_some())),
        days: chosen,
        model: crate::forecast::model_name(series.model.as_str()),
        run_unix_s: series.run.timestamp(),
        fetched_unix_s: response.fetched_at.timestamp(),
        credits: response.attributions.iter().map(credit).collect(),
    })
}

/// What a reading of a model is and is not, for the foot of the screen,
/// with what the thermal figures are where there are any.
fn note(series: &ForecastSeries, thermals: bool) -> String {
    let name = crate::forecast::model_name(series.model.as_str());
    let ground = series
        .cell_elevation_m
        .map(|ground| format!(", whose ground lies at {ground:.0} m"))
        .unwrap_or_default();
    let estimated = if thermals {
        " Thermals are an estimate from the model's heat and temperatures over that cell: \
         expect more on a sunlit slope and less in its lee."
    } else {
        ""
    };
    format!(
        "Read from {name} for its grid cell{ground}. It knows nothing of the take-off itself; \
         the take-off decides.{estimated}"
    )
}

/// What the site's source says about the take-off.
struct Site<'a> {
    wind: &'a [WindSpan],
    /// The same in words: "N to NE". Empty when none are given.
    works_in: String,
    elevation_m: Option<f64>,
}

/// An hour as it is judged, with what the day's line is made from.
struct Judged<'a> {
    hour: &'a ForecastHour,
    local_hour: u32,
    verdict: Flyability,
    why: String,
    /// What the model makes of the hour's thermals over its cell.
    air: Option<Convection>,
    /// What they count as for the day.
    thermals: Option<Thermals>,
}

/// What an hour's thermals count as for a take-off at `launch_m`, in an
/// hour with this verdict.
fn thermals(air: &Convection, launch_m: f64, verdict: Flyability) -> Option<Thermals> {
    let reach = air.top_m - launch_m;
    let calm = |limit_kmh: f64| air.wind_ms.is_none_or(|wind| wind * 3.6 <= limit_kmh);
    let by_the_air = if air.climb_ms >= CLIMB_MS[2] && reach >= REACH_M[2] && calm(DRIFT_KMH[1]) {
        Thermals::Big
    } else if air.climb_ms >= CLIMB_MS[1] && reach >= REACH_M[1] && calm(DRIFT_KMH[0]) {
        Thermals::CrossCountry
    } else if air.climb_ms >= CLIMB_MS[0] && reach >= REACH_M[0] {
        Thermals::Local
    } else {
        Thermals::Weak
    };
    match verdict {
        Flyability::Flyable => Some(by_the_air),
        // Something speaks against the hour: it is none to leave in.
        Flyability::Marginal => Some(by_the_air.min(Thermals::Local)),
        Flyability::NotFlyable => None,
    }
}

/// A height to the nearest fifty metres: the estimate is no finer.
pub(crate) fn fifty(metres: f64) -> f64 {
    (metres / 50.0).round() * 50.0
}

/// A climb to a tenth of a metre per second.
pub(crate) fn tenth(ms: f64) -> f64 {
    (ms * 10.0).round() / 10.0
}

fn days(
    series: &ForecastSeries,
    site: &Site<'_>,
    utc_offset_s: i32,
    now_unix_s: i64,
) -> Vec<OutlookDay> {
    let offset = Duration::seconds(i64::from(utc_offset_s));
    // The current hour is still to come for as long as it lasts.
    let from = now_unix_s - now_unix_s.rem_euclid(3600);
    // Each day with its hours still to come, and whether the model had
    // daylight hours for it that are gone.
    let mut days: Vec<(DateTime<Utc>, Vec<Judged<'_>>, bool)> = Vec::new();
    for hour in &series.hours {
        let local = hour.time + offset;
        let start = local
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map_or(hour.time, |midnight| midnight.and_utc() - offset);
        if days.last().is_none_or(|(day, _, _)| *day != start) {
            days.push((start, Vec::new(), false));
        }
        // The daylight of the day that holds the hour, an hour in from
        // either end: the first and the last hour of the sun are not for
        // a judgement by wind and cloud alone.
        let light = sun::daylight_at(start + Duration::hours(12), series.cell);
        let in_daylight = matches!(
            (light.sunrise, light.sunset),
            (Some(rise), Some(set))
                if hour.time >= rise + Duration::hours(1) && hour.time + Duration::hours(1) <= set
        );
        if !in_daylight {
            continue;
        }
        if hour.time.timestamp() < from {
            if let Some((_, _, gone)) = days.last_mut() {
                *gone = true;
            }
            continue;
        }
        let (verdict, why) = judge(hour, site);
        // The thermals over the model's cell, and what they are worth at
        // the take-off's height; at the model's ground where the take-off
        // has none.
        let air = series
            .cell_elevation_m
            .and_then(|ground| convection(hour, ground));
        let launch = site.elevation_m.or(series.cell_elevation_m);
        let thermals = air
            .zip(launch)
            .and_then(|(air, launch)| thermals(&air, launch, verdict));
        if let Some((_, hours, _)) = days.last_mut() {
            hours.push(Judged {
                hour,
                local_hour: local.hour(),
                verdict,
                why,
                air,
                thermals,
            });
        }
    }
    let today = (DateTime::from_timestamp(now_unix_s, 0).unwrap_or_default() + offset).date_naive();
    let ground = series.cell_elevation_m;
    days.into_iter()
        .filter_map(|(start, hours, gone)| {
            let date = (start + offset).date_naive();
            let ahead = (date - today).num_days();
            // A day that is over is not listed, and neither is a day the
            // model has no daylight hours for. Today stays when its hours
            // are gone: that none are left is worth saying.
            let listed = ahead >= 0 && (!hours.is_empty() || (ahead == 0 && gone));
            let name = match ahead {
                0 => "Today".to_owned(),
                1 => "Tomorrow".to_owned(),
                _ => date.format("%a").to_string(),
            };
            listed.then(|| {
                let mut day = day(start.timestamp(), name, &hours, ground);
                day.model = crate::forecast::model_name(series.model.as_str());
                day.run_unix_s = series.run.timestamp();
                day.note = note(series, day.thermals.is_some());
                day
            })
        })
        .collect()
}

/// What the forecast says about one hour: the worst thing against it.
fn judge(hour: &ForecastHour, site: &Site<'_>) -> (Flyability, String) {
    let kmh = |speed_ms: f64| speed_ms * 3.6;
    let mut against: Vec<(Flyability, String)> = Vec::new();
    let mut weigh = |value: f64, limits: (f64, f64), words: String| {
        if value >= limits.1 {
            against.push((Flyability::NotFlyable, words));
        } else if value >= limits.0 {
            against.push((Flyability::Marginal, words));
        }
    };

    if hour.precipitation_mm.is_some_and(|rain| rain >= RAIN_MM) {
        return (Flyability::NotFlyable, "Rain".to_owned());
    }
    if let Some(speed) = hour.wind_speed_ms.map(kmh) {
        weigh(speed, WIND_KMH, format!("Wind {speed:.0} km/h"));
        // A wind from where the take-off does not work, unless it is
        // light: then thermals and the slope decide, not the model.
        if let Some(from) = hour.wind_direction_deg
            && wind_fits(site.wind.to_vec(), from) == Some(false)
        {
            weigh(
                speed,
                WRONG_WAY_KMH,
                format!(
                    "Wind from {}, take-off works in {}",
                    compass(from),
                    site.works_in
                ),
            );
        }
    }
    if let Some(gusts) = hour.wind_gust_ms.map(kmh) {
        weigh(gusts, GUSTS_KMH, format!("Gusts to {gusts:.0} km/h"));
    }
    if let Some(takeoff) = site.elevation_m {
        // The strongest wind at the model's levels over the take-off.
        let aloft = hour
            .winds_aloft
            .iter()
            .filter_map(|level| Some((level.altitude_m?, kmh(level.wind_speed_ms?))))
            .filter(|(altitude, _)| (takeoff..=takeoff + ALOFT_REACH_M).contains(altitude))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((altitude, speed)) = aloft {
            let at = (altitude / 100.0).round() * 100.0;
            weigh(
                speed,
                ALOFT_KMH,
                format!("Wind {speed:.0} km/h at {at:.0} m"),
            );
        }
        if let Some(base) = hour.convective_cloud_base_m {
            if base < takeoff {
                against.push((
                    Flyability::NotFlyable,
                    "Cloud base below the take-off".to_owned(),
                ));
            } else if base < takeoff + CLOUD_BASE_CLEAR_M {
                against.push((
                    Flyability::Marginal,
                    "Cloud base near the take-off".to_owned(),
                ));
            }
        }
    }
    if hour.cape_j_kg.is_some_and(|cape| cape >= CAPE_J_KG) {
        against.push((
            Flyability::Marginal,
            "Showers or thunderstorms can grow".to_owned(),
        ));
    }
    // The worst, and of two equally bad the one found first.
    let worst = against
        .iter()
        .map(|(verdict, _)| *verdict)
        .max()
        .unwrap_or(Flyability::Flyable);
    against
        .into_iter()
        .find(|(verdict, _)| *verdict == worst)
        .unwrap_or((Flyability::Flyable, String::new()))
}

/// The longest stretch of hours in a row that `counts`, as the indices of
/// its first and its last hour, if it is at least `needed` hours long. The
/// earliest of equally long ones.
fn longest_run(
    hours: &[Judged<'_>],
    needed: usize,
    counts: impl Fn(&Judged<'_>) -> bool,
) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut start = None;
    for (index, judged) in hours.iter().enumerate() {
        // Hours in a row are an hour apart: a gap in the model's hours
        // ends a stretch.
        let follows =
            index > 0 && judged.hour.time - hours[index - 1].hour.time == Duration::hours(1);
        if !counts(judged) {
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

/// The longest stretch of hours in a row that are no worse than `limit`.
fn stretch(hours: &[Judged<'_>], limit: Flyability) -> Option<(usize, usize)> {
    longest_run(hours, WINDOW_HOURS, |judged| judged.verdict <= limit)
}

/// What the day's thermals come to.
struct ThermalDay {
    kind: Thermals,
    line: String,
    climb_ms: f64,
    /// How high they go; none without thermals.
    top_m: Option<f64>,
}

/// The kind of day the hours' thermals make, with its line. `None` when no
/// hour that can be flown has an estimate.
fn thermal_day(hours: &[Judged<'_>]) -> Option<ThermalDay> {
    // The best kind there are enough hours in a row of.
    let found = [Thermals::Big, Thermals::CrossCountry, Thermals::Local]
        .into_iter()
        .zip([THERMAL_HOURS[2], THERMAL_HOURS[1], THERMAL_HOURS[0]])
        .find_map(|(kind, needed)| {
            longest_run(hours, needed, |judged| judged.thermals >= Some(kind))
                .map(|run| (kind, run))
        });
    if let Some((kind, (from, to))) = found {
        let airs: Vec<Convection> = hours[from..=to]
            .iter()
            .filter_map(|judged| judged.air)
            .collect();
        let climb_ms = tenth(airs.iter().map(|air| air.climb_ms).sum::<f64>() / airs.len() as f64);
        let highest = airs
            .iter()
            .max_by(|a, b| a.top_m.total_cmp(&b.top_m))
            .copied()?;
        let top_m = fifty(highest.top_m);
        let to_where = if highest.cloud_base_m.is_some() {
            format!("to a cloud base of {top_m:.0} m")
        } else {
            format!("to {top_m:.0} m in the blue")
        };
        return Some(ThermalDay {
            kind,
            line: format!(
                "{climb_ms:.1} m/s {to_where}, from {} to {}",
                clock(hours[from].local_hour),
                clock(hours[to].local_hour + 1)
            ),
            climb_ms,
            top_m: Some(top_m),
        });
    }
    // Weak: the best there is, among the hours that can be flown.
    let best = hours
        .iter()
        .filter(|judged| judged.thermals.is_some())
        .filter_map(|judged| judged.air)
        .max_by(|a, b| a.climb_ms.total_cmp(&b.climb_ms))?;
    let climb_ms = tenth(best.climb_ms);
    let top_m = (best.depth_m > 0.0).then(|| fifty(best.top_m));
    Some(ThermalDay {
        kind: Thermals::Weak,
        line: match top_m {
            Some(top_m) if climb_ms > 0.0 => format!("{climb_ms:.1} m/s to {top_m:.0} m at best"),
            _ => "None to climb in".to_owned(),
        },
        climb_ms,
        top_m,
    })
}

/// An hour of the clock as a pilot says it: "noon", "15:00".
fn clock(hour: u32) -> String {
    if hour == 12 {
        "noon".to_owned()
    } else {
        format!("{hour:02}:00")
    }
}

/// The wind at the model's levels in an hour, lowest first, with each
/// level's height above the sea.
fn winds(hour: &ForecastHour, ground_m: f64) -> Vec<OutlookWind> {
    let at_the_ground = hour.wind_speed_ms.map(|speed| OutlookWind {
        altitude_m: ground_m + 10.0,
        wind: forecast_wind(speed, None, hour.wind_direction_deg),
    });
    let mut winds: Vec<OutlookWind> = at_the_ground
        .into_iter()
        .chain(hour.winds_aloft.iter().filter_map(|level| {
            let altitude_m = match level.above_ground_m {
                Some(above) => ground_m + above,
                None => level.altitude_m?,
            };
            Some(OutlookWind {
                altitude_m,
                wind: forecast_wind(level.wind_speed_ms?, None, level.wind_direction_deg),
            })
        }))
        .filter(|wind| wind.altitude_m > ground_m && wind.altitude_m <= WINDS_UP_TO_M)
        .collect();
    winds.sort_by(|a, b| a.altitude_m.total_cmp(&b.altitude_m));
    winds
}

fn day(start_unix_s: i64, name: String, hours: &[Judged<'_>], ground_m: Option<f64>) -> OutlookDay {
    let good = stretch(hours, Flyability::Flyable);
    let fair = stretch(hours, Flyability::Marginal);
    let verdict = match (good, fair) {
        (Some(_), _) => Flyability::Flyable,
        (None, Some(_)) => Flyability::Marginal,
        (None, None) => Flyability::NotFlyable,
    };

    // The reason met most often among the hours that are worse than the
    // day's verdict allows; of equally frequent ones the earliest.
    let most_said = |from: Flyability| -> String {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for judged in hours.iter().filter(|judged| judged.verdict >= from) {
            match counts.iter_mut().find(|(why, _)| *why == judged.why) {
                Some((_, count)) => *count += 1,
                None => counts.push((&judged.why, 1)),
            }
        }
        let most = counts.iter().map(|(_, count)| *count).max().unwrap_or(0);
        counts
            .into_iter()
            .find(|(_, count)| *count == most)
            .map(|(why, _)| why.to_owned())
            .unwrap_or_default()
    };

    let (headline, why) = match (verdict, good) {
        (Flyability::Flyable, Some((from, to))) => {
            let begins = hours[from].local_hour;
            let ends = hours[to].local_hour + 1;
            let line = match (from == 0, to == hours.len() - 1) {
                (true, true) => "Good all day".to_owned(),
                (true, false) => format!("Good until {}", clock(ends)),
                (false, true) => format!("Good from {}", clock(begins)),
                (false, false) => format!("Good from {} to {}", clock(begins), clock(ends)),
            };
            // What ends the stretch, else what delays it.
            let why = if let Some(after) = hours.get(to + 1) {
                format!("{} after {}", after.why, clock(ends))
            } else if from > 0 {
                format!("{} until {}", hours[from - 1].why, clock(begins))
            } else {
                String::new()
            };
            (line, why)
        }
        (Flyability::Marginal, _) => ("Marginal".to_owned(), most_said(Flyability::Marginal)),
        _ if hours.is_empty() => ("No flying hours left".to_owned(), String::new()),
        _ => ("Not flyable".to_owned(), most_said(Flyability::NotFlyable)),
    };

    // The thermals of a day that can be flown.
    let thermal = (verdict != Flyability::NotFlyable)
        .then(|| thermal_day(hours))
        .flatten();

    // The wind and the cloud base of the middle of the best stretch there
    // is, else of the middle of the day's hours.
    let middle = good
        .or(fair)
        .map(|(from, to)| (from + to) / 2)
        .or_else(|| (!hours.is_empty()).then_some(hours.len() / 2))
        .map(|index| hours[index].hour);
    OutlookDay {
        start_unix_s,
        name,
        verdict,
        status: verdict.words().to_owned(),
        headline,
        why,
        hours: hours
            .iter()
            .map(|judged| OutlookHour {
                time_unix_s: judged.hour.time.timestamp(),
                hour: judged.local_hour.to_string(),
                verdict: judged.verdict,
                why: judged.why.clone(),
                wind_from_deg: judged.hour.wind_direction_deg,
                wind_kmh: judged
                    .hour
                    .wind_speed_ms
                    .map(|speed| format!("{:.0}", speed * 3.6))
                    .unwrap_or_default(),
                thermals: judged.thermals,
                climb_ms: judged.air.map(|air| tenth(air.climb_ms)),
                thermal_top_m: judged
                    .air
                    .filter(|air| air.depth_m > 0.0)
                    .map(|air| fifty(air.top_m)),
                thermal_cloud: judged.air.is_some_and(|air| air.cloud_base_m.is_some()),
                winds: ground_m
                    .map(|ground| winds(judged.hour, ground))
                    .unwrap_or_default(),
            })
            .collect(),
        wind_kmh: middle
            .and_then(|hour| hour.wind_speed_ms)
            .map(|speed| format!("{:.0}", speed * 3.6))
            .unwrap_or_default(),
        gust_kmh: middle
            .and_then(|hour| hour.wind_gust_ms)
            .map(|gusts| format!("{:.0}", gusts * 3.6))
            .unwrap_or_default(),
        wind_from: middle
            .and_then(|hour| hour.wind_direction_deg)
            .map(|from| compass(from).to_owned())
            .unwrap_or_default(),
        cloud_base_m: middle.and_then(|hour| hour.convective_cloud_base_m),
        thermals: thermal.as_ref().map(|thermal| thermal.kind),
        thermal_words: thermal
            .as_ref()
            .map(|thermal| thermal.kind.words().to_owned())
            .unwrap_or_default(),
        thermal_line: thermal
            .as_ref()
            .map(|thermal| thermal.line.clone())
            .unwrap_or_default(),
        climb_ms: thermal.as_ref().map(|thermal| thermal.climb_ms),
        thermal_top_m: thermal.as_ref().and_then(|thermal| thermal.top_m),
        ground_m,
        // Which model the hours are from is the caller's to say.
        model: String::new(),
        run_unix_s: 0,
        note: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A summer day over the Tegernsee valley, where the clock is two
    /// hours ahead of UTC and the sun is up from about half past five to
    /// a quarter past nine: one hour of the model for every hour of the
    /// clock from 8 to 19, each as the test wants it.
    fn answer_with(hours: &[(u32, &str)]) -> Vec<u8> {
        let hours: Vec<String> = hours
            .iter()
            .map(|(local, values)| {
                format!(
                    r#"{{"time": "2026-07-04T{:02}:00:00Z"{}{values}}}"#,
                    local - 2,
                    if values.is_empty() { "" } else { ", " }
                )
            })
            .collect();
        format!(
            r#"{{
            "point": {{"lat": 47.7, "lon": 11.8}},
            "fetched_at": "2026-07-04T05:40:00Z",
            "layers": [{{"source": "open-meteo", "series": [
                {{"source": "open-meteo", "model": "icon-eu", "run": "2026-07-04T00:00:00Z",
                  "point": {{"lat": 47.7, "lon": 11.8}}, "cell": {{"lat": 47.6875, "lon": 11.8125}},
                  "hours": [{{"time": "2026-07-04T10:00:00Z", "wind_speed_ms": 20.0}}]}},
                {{"source": "open-meteo", "model": "icon-d2", "run": "2026-07-04T03:00:00Z",
                  "point": {{"lat": 47.7, "lon": 11.8}}, "cell": {{"lat": 47.7, "lon": 11.8}},
                  "cell_elevation_m": 812.0,
                  "hours": [{}]}}
            ]}}],
            "attributions": [{{"source": "open-meteo", "provider": "Open-Meteo", "licence": "CC BY 4.0",
                              "text": "Weather data by Open-Meteo.com", "url": "https://open-meteo.com/"}}]
        }}"#,
            hours.join(",")
        )
        .into_bytes()
    }

    const SUMMER_TIME_S: i32 = 2 * 3600;
    /// Six in the morning by the valley's clock on 4 July 2026.
    const MORNING_S: i64 = 1_783_137_600;

    /// A north wind of 3 m/s with gusts of 5: what a take-off that faces
    /// north wants.
    const FINE: &str = r#""wind_speed_ms": 3.0, "wind_gust_ms": 5.0, "wind_direction_deg": 0.0,
        "precipitation_mm": 0.0, "convective_cloud_base_m": 2900.0, "cape_j_kg": 150.0"#;

    fn north() -> Vec<WindSpan> {
        vec![WindSpan {
            from_deg: 337.5,
            to_deg: 22.5,
        }]
    }

    fn outlook(hours: &[(u32, &str)]) -> SiteOutlook {
        site_outlook(
            200,
            answer_with(hours),
            SUMMER_TIME_S,
            north(),
            Some(1550.0),
            MORNING_S,
        )
        .unwrap()
    }

    #[test]
    fn a_day_with_nothing_against_it_is_good_all_day() {
        let hours: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, FINE)).collect();
        let outlook = outlook(&hours);
        assert_eq!(outlook.model, "ICON-D2");
        assert_eq!(
            outlook.note,
            "Read from ICON-D2 for its grid cell, whose ground lies at 812 m. It knows nothing of \
             the take-off itself; the take-off decides."
        );
        assert_eq!(outlook.credits.len(), 1);
        let day = &outlook.days[0];
        assert_eq!(day.verdict, Flyability::Flyable);
        assert_eq!(day.status, "Flyable");
        assert_eq!(day.headline, "Good all day");
        assert_eq!(day.why, "");
        assert_eq!(day.hours.len(), 12);
        assert_eq!(day.hours[0].hour, "8");
        assert_eq!(day.hours[0].wind_kmh, "11");
        assert_eq!(day.wind_kmh, "11");
        assert_eq!(day.gust_kmh, "18");
        assert_eq!(day.wind_from, "N");
        assert_eq!(day.cloud_base_m, Some(2900.0));
    }

    /// An hour of FINE's wind with the air that makes thermals: the heat
    /// the ground gives, the model's own cloud base (zero for none), the
    /// dew point at the ground and the wind at the levels above. 26 °C at
    /// the model's ground at 812 m, well mixed to 1500 m, and air that
    /// stops a parcel from the ground at 3250 m.
    fn air(heat: f64, base_m: f64, dew_c: f64, aloft_ms: f64) -> String {
        let base = if base_m > 0.0 {
            format!(r#""convective_cloud_base_m": {base_m:.1}, "#)
        } else {
            String::new()
        };
        format!(
            r#""wind_speed_ms": 3.0, "wind_gust_ms": 5.0, "wind_direction_deg": 0.0,
            "precipitation_mm": 0.0, "cape_j_kg": 150.0, {base}"temperature_c": 26.0,
            "dew_point_c": {dew_c:.1}, "heat_to_air_w_m2": {heat:.1}, "winds_aloft": [
            {{"above_ground_m": 80.0, "wind_speed_ms": {aloft_ms:.1}, "temperature_c": 24.0}},
            {{"above_ground_m": 180.0, "wind_speed_ms": {aloft_ms:.1}, "temperature_c": 23.0}},
            {{"pressure_hpa": 850.0, "altitude_m": 1500.0, "wind_speed_ms": {aloft_ms:.1}, "temperature_c": 18.0}},
            {{"pressure_hpa": 700.0, "altitude_m": 3100.0, "wind_speed_ms": {aloft_ms:.1}, "temperature_c": 3.0}},
            {{"pressure_hpa": 600.0, "altitude_m": 4300.0, "wind_speed_ms": {aloft_ms:.1}, "temperature_c": -4.0}}]"#
        )
    }

    /// The outlook of a day whose hours from 8 to 19 are each what `hour`
    /// makes of them.
    fn outlook_of(hour: impl Fn(u32) -> String) -> SiteOutlook {
        let hours: Vec<(u32, String)> = (8..=19).map(|at| (at, hour(at))).collect();
        let hours: Vec<(u32, &str)> = hours
            .iter()
            .map(|(at, values)| (*at, values.as_str()))
            .collect();
        outlook(&hours)
    }

    #[test]
    fn strong_high_thermals_for_most_of_the_day_make_a_big_day() {
        // The sun's heat through the day, and cumulus at 3400 m.
        let outlook = outlook_of(|hour| {
            let heat = match hour {
                8 | 19 => 90.0,
                9 | 18 => 200.0,
                10 => 300.0,
                _ => 400.0,
            };
            air(heat, 3400.0, 10.0, 4.0)
        });
        let day = &outlook.days[0];
        assert_eq!(day.verdict, Flyability::Flyable);
        assert_eq!(day.headline, "Good all day");

        // Seven hours in a row of 2.2 m/s to 1850 m above the take-off.
        assert_eq!(day.thermals, Some(Thermals::Big));
        assert_eq!(day.thermal_words, "Big day");
        assert_eq!(
            day.thermal_line,
            "2.2 m/s to a cloud base of 3400 m, from 11:00 to 18:00"
        );
        assert_eq!(day.climb_ms, Some(2.2));
        assert_eq!(day.thermal_top_m, Some(3400.0));

        // Hour by hour: weaker in the morning and the evening.
        let kinds: Vec<Option<Thermals>> = day.hours.iter().map(|hour| hour.thermals).collect();
        let (local, away, big) = (
            Some(Thermals::Local),
            Some(Thermals::CrossCountry),
            Some(Thermals::Big),
        );
        assert_eq!(
            kinds,
            [
                local, away, away, big, big, big, big, big, big, big, away, local
            ]
        );
        assert_eq!(day.hours[0].climb_ms, Some(0.9));
        assert_eq!(day.hours[3].climb_ms, Some(2.2));
        assert_eq!(day.hours[3].thermal_top_m, Some(3400.0));

        // The foot of the screen says what the figures are.
        assert!(outlook.note.ends_with(
            "Thermals are an estimate from the model's heat and temperatures over that cell: \
             expect more on a sunlit slope and less in its lee."
        ));
    }

    #[test]
    fn an_hour_carries_the_air_over_the_take_off_for_a_picture_of_the_day() {
        // Cumulus at 3400 m, and 4 m/s at every level above the ground.
        let outlook = outlook_of(|_| air(400.0, 3400.0, 10.0, 4.0));
        let day = &outlook.days[0];
        // Where the model's thermals begin: its own ground, not the
        // take-off's 1550 m.
        assert_eq!(day.ground_m, Some(812.0));
        let noon = &day.hours[4];
        assert_eq!(noon.thermal_top_m, Some(3400.0));
        assert!(noon.thermal_cloud);
        // The wind 10 m above the model's ground, then at its levels by
        // their height above the sea, lowest first.
        let heights: Vec<f64> = noon.winds.iter().map(|wind| wind.altitude_m).collect();
        assert_eq!(heights, [822.0, 892.0, 992.0, 1500.0, 3100.0, 4300.0]);
        assert_eq!(noon.winds[0].wind.label, "11");
        assert_eq!(noon.winds[0].wind.from_deg, Some(0.0));
        assert_eq!(noon.winds[3].wind.label, "14");
        assert_eq!(noon.winds[3].wind.from_deg, None);

        // A blue day has no cloud on its thermals.
        let day = outlook_of(|_| air(300.0, 0.0, 4.0, 4.0)).days.remove(0);
        assert_eq!(day.hours[4].thermal_top_m, Some(3250.0));
        assert!(!day.hours[4].thermal_cloud);

        // A level above 5000 m is not the air a paraglider flies in.
        let day = outlook_of(|_| {
            air(300.0, 0.0, 4.0, 4.0).replace(r#""altitude_m": 4300.0"#, r#""altitude_m": 5600.0"#)
        })
        .days
        .remove(0);
        assert_eq!(day.hours[4].winds.len(), 5);
    }

    #[test]
    fn a_day_is_read_from_the_model_that_has_the_most_of_it() {
        // ICON-D2 has all of 4 July and the morning of the 5th, where its
        // run ends. ICON-EU has the three days whole.
        let hours = |day: u32, local: std::ops::RangeInclusive<u32>| -> String {
            local
                .map(|hour| {
                    format!(
                        r#"{{"time": "2026-07-{day:02}T{:02}:00:00Z", {FINE}}}"#,
                        hour - 2
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let fine = [hours(4, 8..=19), hours(5, 8..=10)].join(",");
        let coarse = [hours(4, 8..=19), hours(5, 8..=19), hours(6, 8..=19)].join(",");
        let body = format!(
            r#"{{
            "point": {{"lat": 47.7, "lon": 11.8}},
            "fetched_at": "2026-07-04T05:40:00Z",
            "layers": [{{"source": "open-meteo", "series": [
                {{"source": "open-meteo", "model": "icon-eu", "run": "2026-07-04T00:00:00Z",
                  "point": {{"lat": 47.7, "lon": 11.8}}, "cell": {{"lat": 47.6875, "lon": 11.8125}},
                  "cell_elevation_m": 900.0, "hours": [{coarse}]}},
                {{"source": "open-meteo", "model": "icon-d2", "run": "2026-07-04T03:00:00Z",
                  "point": {{"lat": 47.7, "lon": 11.8}}, "cell": {{"lat": 47.7, "lon": 11.8}},
                  "cell_elevation_m": 812.0, "hours": [{fine}]}}
            ]}}],
            "attributions": []
        }}"#
        );
        let outlook = site_outlook(
            200,
            body.into_bytes(),
            SUMMER_TIME_S,
            north(),
            Some(1550.0),
            MORNING_S,
        )
        .unwrap();

        // Today is the fine model's: both have all of it, and the finer
        // one is believed. Tomorrow, of which it has three hours, and the
        // day after, of which it has none, are the coarse model's.
        let read: Vec<(&str, &str, usize)> = outlook
            .days
            .iter()
            .map(|day| (day.name.as_str(), day.model.as_str(), day.hours.len()))
            .collect();
        assert_eq!(
            read,
            [
                ("Today", "ICON-D2", 12),
                ("Tomorrow", "ICON-EU", 12),
                ("Mon", "ICON-EU", 12)
            ]
        );
        // Each day says whose reading it is, with that model's own run and
        // ground; the outlook as a whole names the finest.
        assert_eq!(outlook.model, "ICON-D2");
        assert_eq!(outlook.days[0].ground_m, Some(812.0));
        assert!(outlook.days[0].note.starts_with("Read from ICON-D2"));
        assert_eq!(outlook.days[1].ground_m, Some(900.0));
        assert_eq!(outlook.days[1].run_unix_s, 1_783_123_200);
        assert_eq!(
            outlook.days[1].note,
            "Read from ICON-EU for its grid cell, whose ground lies at 900 m. It knows nothing of \
             the take-off itself; the take-off decides."
        );
    }

    #[test]
    fn four_good_hours_make_a_cross_country_day_and_two_a_day_of_local_thermals() {
        // Noon to five with 300 W/m² under cumulus at 2900 m: 1.7 m/s to
        // 1350 m above the take-off. Strong, and not high enough for more.
        let day = outlook_of(|hour| {
            air(
                if (12..=16).contains(&hour) {
                    300.0
                } else {
                    60.0
                },
                2900.0,
                10.0,
                4.0,
            )
        })
        .days
        .remove(0);
        assert_eq!(day.thermals, Some(Thermals::CrossCountry));
        assert_eq!(day.thermal_words, "Cross-country day");
        assert_eq!(
            day.thermal_line,
            "1.7 m/s to a cloud base of 2900 m, from noon to 17:00"
        );

        // Only three such hours: the day is one of local thermals, and its
        // line is about all the hours there is something to climb in.
        let day = outlook_of(|hour| {
            air(
                if (12..=14).contains(&hour) {
                    300.0
                } else {
                    60.0
                },
                2900.0,
                10.0,
                4.0,
            )
        })
        .days
        .remove(0);
        assert_eq!(day.thermals, Some(Thermals::Local));
        assert_eq!(day.thermal_words, "Local thermals");
        assert_eq!(
            day.thermal_line,
            "0.8 m/s to a cloud base of 2900 m, from 08:00 to 20:00"
        );
    }

    #[test]
    fn dry_air_gives_blue_thermals_to_where_they_end() {
        // No convective cloud in the model, and 22 degrees between
        // temperature and dew point: the condensation level lies above
        // where the thermals stop.
        let day = outlook_of(|_| air(300.0, 0.0, 4.0, 4.0)).days.remove(0);
        assert_eq!(day.thermals, Some(Thermals::CrossCountry));
        assert_eq!(
            day.thermal_line,
            "1.8 m/s to 3250 m in the blue, from 08:00 to 20:00"
        );
        assert_eq!(day.thermal_top_m, Some(3250.0));
        // The model's own cloud base is a different figure and stays empty.
        assert_eq!(day.cloud_base_m, None);
    }

    #[test]
    fn wind_in_the_thermals_and_a_marginal_hour_keep_a_day_local() {
        // 29 km/h at the levels the thermals pass through: strong climbs,
        // and no day to leave with. The wind itself is within the limits.
        let day = outlook_of(|_| air(400.0, 3400.0, 10.0, 8.0)).days.remove(0);
        assert_eq!(day.verdict, Flyability::Flyable);
        assert_eq!(day.thermals, Some(Thermals::Local));
        assert_eq!(day.hours[4].climb_ms, Some(2.2));

        // Gusts that make every hour marginal: the same.
        let day = outlook_of(|_| {
            air(400.0, 3400.0, 10.0, 4.0)
                .replace(r#""wind_gust_ms": 5.0"#, r#""wind_gust_ms": 8.7"#)
        })
        .days
        .remove(0);
        assert_eq!(day.verdict, Flyability::Marginal);
        assert_eq!(day.thermals, Some(Thermals::Local));
        assert!(
            day.hours
                .iter()
                .all(|hour| hour.thermals == Some(Thermals::Local))
        );
    }

    #[test]
    fn weak_thermals_and_thermals_under_the_take_off_are_said_as_they_are() {
        // A hazy day: 20 W/m² lift the air at 1.1 m/s, a glider at 0.1.
        let day = outlook_of(|_| air(20.0, 0.0, 4.0, 4.0)).days.remove(0);
        assert_eq!(day.thermals, Some(Thermals::Weak));
        assert_eq!(day.thermal_words, "Weak thermals");
        assert_eq!(day.thermal_line, "0.1 m/s to 3250 m at best");

        // The ground gives no heat: nothing rises.
        let day = outlook_of(|_| air(-10.0, 0.0, 4.0, 4.0)).days.remove(0);
        assert_eq!(day.thermals, Some(Thermals::Weak));
        assert_eq!(day.thermal_line, "None to climb in");
        assert_eq!(day.climb_ms, Some(0.0));
        assert_eq!(day.thermal_top_m, None);
        assert_eq!(day.hours[0].climb_ms, Some(0.0));
        assert_eq!(day.hours[0].thermal_top_m, None);

        // Cumulus at 1700 m, 150 m over the take-off at 1550 m: thermals,
        // and none to get away from the hill in.
        let day = outlook_of(|_| air(300.0, 1700.0, 10.0, 4.0)).days.remove(0);
        assert_eq!(day.thermals, Some(Thermals::Weak));
        assert_eq!(day.thermal_line, "1.0 m/s to 1700 m at best");
    }

    #[test]
    fn a_day_that_is_not_flyable_and_a_model_without_the_figures_have_no_thermals() {
        // Rain all day, whatever the air would do.
        let day = outlook_of(|_| {
            air(400.0, 3400.0, 10.0, 4.0)
                .replace(r#""precipitation_mm": 0.0"#, r#""precipitation_mm": 0.6"#)
        })
        .days
        .remove(0);
        assert_eq!(day.verdict, Flyability::NotFlyable);
        assert_eq!(day.thermals, None);
        assert_eq!(day.thermal_words, "");
        assert_eq!(day.thermal_line, "");
        assert_eq!(day.climb_ms, None);
        // The hours still say what the air does; they count as nothing.
        assert_eq!(day.hours[4].thermals, None);
        assert_eq!(day.hours[4].climb_ms, Some(2.2));

        // A model that gives no heat from the ground: no estimate, and no
        // word about one at the foot.
        let hours: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, FINE)).collect();
        let outlook = outlook(&hours);
        let day = &outlook.days[0];
        assert_eq!(day.thermals, None);
        assert_eq!(day.thermal_line, "");
        assert_eq!(day.hours[4].climb_ms, None);
        assert!(!outlook.note.contains("Thermals"));
    }

    #[test]
    fn the_day_in_a_line_names_when_it_is_good_and_what_ends_it() {
        const GUSTY: &str =
            r#""wind_speed_ms": 4.0, "wind_gust_ms": 8.7, "wind_direction_deg": 0.0"#;
        const RAIN: &str = r#""wind_speed_ms": 2.0, "precipitation_mm": 0.6"#;
        // Rain until eleven, fine from noon to three, gusts after.
        let hours: Vec<(u32, &str)> = (8..=19)
            .map(|hour| match hour {
                8..=11 => (hour, RAIN),
                12..=14 => (hour, FINE),
                _ => (hour, GUSTY),
            })
            .collect();
        let day = &outlook(&hours).days[0];
        assert_eq!(day.verdict, Flyability::Flyable);
        assert_eq!(day.headline, "Good from noon to 15:00");
        assert_eq!(day.why, "Gusts to 31 km/h after 15:00");
        assert_eq!(day.hours[0].verdict, Flyability::NotFlyable);
        assert_eq!(day.hours[0].why, "Rain");
        assert_eq!(day.hours[7].verdict, Flyability::Marginal);

        // Fine from noon to the evening.
        let hours: Vec<(u32, &str)> = (8..=19)
            .map(|hour| (hour, if hour < 12 { RAIN } else { FINE }))
            .collect();
        let day = &outlook(&hours).days[0];
        assert_eq!(day.headline, "Good from noon");
        assert_eq!(day.why, "Rain until noon");

        // Fine until two.
        let hours: Vec<(u32, &str)> = (8..=19)
            .map(|hour| (hour, if hour < 14 { FINE } else { GUSTY }))
            .collect();
        let day = &outlook(&hours).days[0];
        assert_eq!(day.headline, "Good until 14:00");
        assert_eq!(day.why, "Gusts to 31 km/h after 14:00");
    }

    #[test]
    fn a_wind_from_the_wrong_side_counts_once_it_is_more_than_light() {
        let south =
            |speed: f64| format!(r#""wind_speed_ms": {speed}, "wind_direction_deg": 180.0"#);
        // 6 km/h from behind: light, the model is not asked.
        let light = south(1.7);
        let hours: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, light.as_str())).collect();
        assert_eq!(outlook(&hours).days[0].verdict, Flyability::Flyable);

        // 11 km/h from behind: marginal, and the day says why.
        let more = south(3.0);
        let hours: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, more.as_str())).collect();
        let day = &outlook(&hours).days[0];
        assert_eq!(day.verdict, Flyability::Marginal);
        assert_eq!(day.headline, "Marginal");
        assert_eq!(day.why, "Wind from S, take-off works in NNW to NNE");

        // 18 km/h from behind: not flyable.
        let strong = south(5.0);
        let hours: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, strong.as_str())).collect();
        let day = &outlook(&hours).days[0];
        assert_eq!(day.verdict, Flyability::NotFlyable);
        assert_eq!(day.status, "Not flyable");
        assert_eq!(day.headline, "Not flyable");

        // A take-off whose source names no directions is not judged by
        // the direction.
        let free = site_outlook(
            200,
            answer_with(&hours),
            SUMMER_TIME_S,
            Vec::new(),
            None,
            MORNING_S,
        )
        .unwrap();
        assert_eq!(free.days[0].verdict, Flyability::Flyable);
    }

    #[test]
    fn strong_wind_higher_up_and_low_cloud_speak_against_an_hour() {
        // Calm in the valley cell, 43 km/h at 2400 m over a take-off at
        // 1550 m.
        const FOEHN: &str = r#""wind_speed_ms": 1.5, "wind_direction_deg": 0.0, "winds_aloft": [
            {"pressure_hpa": 850.0, "altitude_m": 1530.0, "wind_speed_ms": 20.0},
            {"pressure_hpa": 750.0, "altitude_m": 2440.0, "wind_speed_ms": 12.0},
            {"pressure_hpa": 600.0, "altitude_m": 4200.0, "wind_speed_ms": 30.0}]"#;
        let hours: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, FOEHN)).collect();
        let day = &outlook(&hours).days[0];
        assert_eq!(day.verdict, Flyability::Marginal);
        // The level below the take-off and the one far above do not count.
        assert_eq!(day.why, "Wind 43 km/h at 2400 m");

        const IN_CLOUD: &str = r#""wind_speed_ms": 1.5, "convective_cloud_base_m": 1400.0"#;
        let hours: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, IN_CLOUD)).collect();
        let day = &outlook(&hours).days[0];
        assert_eq!(day.verdict, Flyability::NotFlyable);
        assert_eq!(day.why, "Cloud base below the take-off");

        const STORMY: &str = r#""wind_speed_ms": 1.5, "cape_j_kg": 1200.0"#;
        let hours: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, STORMY)).collect();
        let day = &outlook(&hours).days[0];
        assert_eq!(day.verdict, Flyability::Marginal);
        assert_eq!(day.why, "Showers or thunderstorms can grow");
    }

    #[test]
    fn hours_gone_and_hours_of_the_night_are_left_out() {
        let hours: Vec<(u32, &str)> = (4..=23).map(|hour| (hour, FINE)).collect();
        let whole = site_outlook(
            200,
            answer_with(&hours),
            SUMMER_TIME_S,
            north(),
            Some(1550.0),
            MORNING_S,
        )
        .unwrap();
        // The sun rises at about 5:20 and sets at about 21:15: the hours
        // from 7 to 20 are an hour in from either end.
        let listed: Vec<&str> = whole.days[0]
            .hours
            .iter()
            .map(|hour| hour.hour.as_str())
            .collect();
        assert_eq!(listed.first(), Some(&"7"));
        assert_eq!(listed.last(), Some(&"20"));

        // At a quarter past three in the afternoon the hours before
        // three are gone.
        let afternoon = site_outlook(
            200,
            answer_with(&hours),
            SUMMER_TIME_S,
            north(),
            Some(1550.0),
            MORNING_S + 9 * 3600 + 900,
        )
        .unwrap();
        assert_eq!(afternoon.days[0].hours[0].hour, "15");
        assert_eq!(afternoon.days[0].headline, "Good all day");

        // At night nothing is left of the day.
        let night = site_outlook(
            200,
            answer_with(&hours),
            SUMMER_TIME_S,
            north(),
            Some(1550.0),
            MORNING_S + 17 * 3600,
        )
        .unwrap();
        assert!(night.days[0].hours.is_empty());
        assert_eq!(night.days[0].verdict, Flyability::NotFlyable);
        assert_eq!(night.days[0].headline, "No flying hours left");
    }

    #[test]
    fn a_day_is_named_by_the_calendar_and_a_day_that_is_over_is_not_listed() {
        let hours: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, FINE)).collect();
        // The forecast is for Saturday 4 July. Days are counted on the
        // place's calendar from the day the clock shows.
        let seen_from = |days: i64| {
            site_outlook(
                200,
                answer_with(&hours),
                SUMMER_TIME_S,
                north(),
                Some(1550.0),
                MORNING_S + days * 86_400,
            )
            .unwrap()
            .days
            .into_iter()
            .map(|day| day.name)
            .collect::<Vec<_>>()
        };
        assert_eq!(seen_from(0), ["Today"]);
        // Seen from the day before, the forecast does not begin today:
        // its first day is tomorrow and is called that.
        assert_eq!(seen_from(-1), ["Tomorrow"]);
        assert_eq!(seen_from(-2), ["Sat"]);
        // Seen from the day after, an old forecast has nothing to say.
        assert!(seen_from(1).is_empty());

        // The fine model has the night alone, which is no day to speak
        // of: not even today, of which it would be wrong to say that no
        // hours are left. The day is then the coarser model's, which has
        // its one hour at noon with a storm's wind, and says whose it is.
        let night = site_outlook(
            200,
            answer_with(&[(23, FINE)]),
            SUMMER_TIME_S,
            north(),
            Some(1550.0),
            MORNING_S,
        )
        .unwrap();
        assert_eq!(night.model, "ICON-D2");
        assert_eq!(night.days.len(), 1);
        assert_eq!(night.days[0].model, "ICON-EU");
        assert_eq!(night.days[0].hours.len(), 1);
        assert_eq!(night.days[0].verdict, Flyability::NotFlyable);
    }

    #[test]
    fn a_place_without_a_forecast_has_no_days() {
        let empty = br#"{"point": {"lat": 47.7, "lon": 11.8}, "fetched_at": "2026-07-04T05:40:00Z",
            "layers": [], "attributions": []}"#;
        let outlook =
            site_outlook(200, empty.to_vec(), SUMMER_TIME_S, north(), None, MORNING_S).unwrap();
        assert!(outlook.days.is_empty());
        assert_eq!(outlook.model, "");
        assert!(site_outlook(503, Vec::new(), 0, Vec::new(), None, 0).is_err());
    }
}
