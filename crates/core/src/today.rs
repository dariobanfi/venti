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
//! every limit is plain to read, and the view names the model, its run
//! and its cell. The limits are our own, round numbers a cautious pilot
//! would nod to. To verify with pilots, all of them.
//!
//! Two things stand beside the verdicts and change none of them. The
//! strongest wind and gust the model has in the cells around the place,
//! which the server hands out with the forecast, with a line when that is
//! much more than the place has ([`venti_flight::nearby`]): a foehn two
//! valleys away is in those cells before it is here. And the layering of
//! the air: where between two of the model's levels the temperature falls
//! too slowly for thermals, or rises ([`venti_flight::layering`]).
//!
//! The rules that judge by the air alone, with their limits, are the
//! flight engine's ([`venti_flight::day`]), which the server's thermal map
//! uses too. What a take-off adds is here: the directions it works in, and
//! the words.

use chrono::{DateTime, Duration, Timelike, Utc};
use venti_api_types::v1::{ForecastNearby, ForecastResponse, NearbyHour, NearbyStrongest};
use venti_domain::{ForecastHour, ForecastSeries, GeoPoint};
use venti_flight::convection::{Convection, convection};
use venti_flight::day::{
    self as rating, Against, Kind, RatedHour, Verdict, against, kind_of_hour, worst,
};
use venti_flight::layering::{self, Stability};
use venti_flight::nearby::{self, Stronger};
use venti_flight::sun;

use crate::airspace::compass;
use crate::forecast::wind as forecast_wind;
use crate::server::answer;
use crate::sites::{WindSpan, wind_fits, wind_spans_in_words};
use crate::wind::credit;
use crate::{CoreError, Credit, ForecastWind};

/// A wind that does not come from a direction the take-off works in:
/// below the first it is light enough not to count, from the second it
/// makes the hour not flyable, in between marginal. km/h.
const WRONG_WAY_KMH: (f64, f64) = (8.0, 15.0);
/// Up to this height above sea level an hour lists the wind at the
/// model's levels, metres: the air a paraglider flies in.
const WINDS_UP_TO_M: f64 = 5000.0;
/// How a take-off looks for flying, by a forecast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
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
    pub(crate) fn words(self) -> &'static str {
        match self {
            Self::Flyable => "Flyable",
            Self::Marginal => "Marginal",
            Self::NotFlyable => "Not flyable",
        }
    }
}

impl From<Verdict> for Flyability {
    fn from(verdict: Verdict) -> Self {
        match verdict {
            Verdict::Flyable => Self::Flyable,
            Verdict::Marginal => Self::Marginal,
            Verdict::NotFlyable => Self::NotFlyable,
        }
    }
}

impl From<Flyability> for Verdict {
    fn from(flyability: Flyability) -> Self {
        match flyability {
            Flyability::Flyable => Self::Flyable,
            Flyability::Marginal => Self::Marginal,
            Flyability::NotFlyable => Self::NotFlyable,
        }
    }
}

/// What thermals a forecast has for a take-off: the kind of day it makes.
///
/// Read from the model's heat and temperatures by the limits above, which
/// are our own. An estimate of the air over a grid cell, never a promise
/// of a climb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
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
    pub(crate) fn words(self) -> &'static str {
        match self {
            Self::Weak => "Weak thermals",
            Self::Local => "Local thermals",
            Self::CrossCountry => "Cross-country day",
            Self::Big => "Big day",
        }
    }
}

impl From<Kind> for Thermals {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Weak => Self::Weak,
            Kind::Local => Self::Local,
            Kind::CrossCountry => Self::CrossCountry,
            Kind::Big => Self::Big,
        }
    }
}

impl From<Thermals> for Kind {
    fn from(thermals: Thermals) -> Self {
        match thermals {
            Thermals::Weak => Self::Weak,
            Thermals::Local => Self::Local,
            Thermals::CrossCountry => Self::CrossCountry,
            Thermals::Big => Self::Big,
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

/// The strongest wind the model has near a take-off in an hour: among
/// the squares of the server's forecast map within 30 km of the place,
/// each of which is one cell of the model. It stands beside the hour and
/// is no part of its verdict.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OutlookNearby {
    /// The strongest mean wind near by, 10 m above the model's ground, in
    /// whole km/h: "38". Empty when no square has a wind in the hour.
    pub wind_kmh: String,
    /// Where that is from the place: "25 km to the south", and "at the
    /// place" for the square the place lies in the middle of. Empty
    /// without a wind.
    pub wind_where: String,
    /// How high the model's ground lies there, metres above sea level. A
    /// square on a ridge has more wind than a valley on any day.
    pub wind_ground_m: Option<f64>,
    /// The strongest gust near by in whole km/h: "55". Empty when no
    /// square has gusts in the hour.
    pub gust_kmh: String,
    /// Where that is: "25 km to the south". Empty without gusts.
    pub gust_where: String,
    /// How high the model's ground lies there.
    pub gust_ground_m: Option<f64>,
    /// Whether that is much stronger than the model has at the place
    /// itself: gusts of 40 km/h or a wind of 30 km/h near by, and 15 km/h
    /// more than at the place.
    pub stronger: bool,
    /// What is much stronger, in a line: "Gusts to 55 km/h 25 km to the
    /// south over ground at 2400 m, 22 km/h at the place". Empty unless
    /// `stronger`.
    pub line: String,
}

/// A stretch of air over a take-off in which thermals stop: between two
/// of the model's levels the temperature falls too slowly, or rises.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OutlookLayer {
    /// Its lower level, metres above sea level to the nearest fifty.
    pub bottom_m: f64,
    /// Its upper level, likewise.
    pub top_m: f64,
    /// Whether the temperature rises with height in it. Otherwise it
    /// falls by less than 0.5 °C per 100 m.
    pub inversion: bool,
    /// By how much the temperature falls with height in it, °C per 100 m
    /// to a tenth: the mean between the two levels. Negative in an
    /// inversion.
    pub fall_c_per_100m: f64,
    /// The layer in words: "Stable layer between 1500 and 3000 m: 0.3 °C
    /// per 100 m", "Inversion between 1000 and 1200 m: 2.0 °C warmer per
    /// 100 m".
    pub words: String,
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
    /// Where thermals stop by the temperatures at the model's levels,
    /// lowest first, up to 5000 m above the sea. Empty where the air
    /// cools fast enough all the way up, and where the model's ground is
    /// not known. Coarse: [`SiteOutlook::layers_note`] says how.
    pub layers: Vec<OutlookLayer>,
    /// The strongest wind and gust the model has near the place in the
    /// hour. `None` when the server gave none: it makes no forecast map,
    /// or the map has no square near the place or not this hour.
    pub nearby: Option<OutlookNearby>,
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
    /// "Marginal", "Not flyable". On a day the model's run ends in with
    /// daylight left, a good stretch that lasts to the run's last hour is
    /// not said to last all day: the line names the hour, "Good until
    /// 14:00".
    pub headline: String,
    /// What limits the day, in words: "Gusts to 31 km/h after 15:00",
    /// "Rain", and where a good stretch lasts to the end of the model's
    /// run and not of the day, "The forecast ends at 14:00". Empty when
    /// nothing does.
    pub why: String,
    /// The wind near by, when in an hour still to come it is much
    /// stronger than at the place: the hour with the strongest of it,
    /// "Much stronger near by at 14:00: gusts to 55 km/h 25 km to the
    /// south over ground at 2400 m, 22 km/h at the place". Empty when no hour's
    /// is. A line to show with the day whatever its verdict; it is no part
    /// of the verdict.
    pub nearby: String,
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
    /// screen while this day is shown. On a day the model's run ends in
    /// with daylight left it also says when: the day is judged by the
    /// hours before, and nothing is known of those after.
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
    /// What the wind near by is and is not, to show wherever an hour's or
    /// a day's is shown. Empty when the server gave none.
    pub nearby_note: String,
    /// What the layers of an hour are and are not, to show wherever they
    /// are drawn or listed. Empty without days.
    pub layers_note: String,
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
            nearby_note: String::new(),
            layers_note: String::new(),
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
    let around = response.nearby.as_ref().map(|nearby| Around {
        place: response.point,
        nearby,
    });
    let mut chosen: Vec<OutlookDay> = Vec::new();
    for model in &models {
        for day in days(model, &site, around.as_ref(), utc_offset_s, now_unix_s) {
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
        nearby_note: response
            .nearby
            .as_ref()
            .map(nearby_note)
            .unwrap_or_default(),
        layers_note: if chosen.is_empty() {
            String::new()
        } else {
            LAYERS_NOTE.to_owned()
        },
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

/// What the layers of an hour are and are not.
const LAYERS_NOTE: &str = "Layers are read between the model's levels as Venti has them: one step \
    from the ground to 180 m above it, then surfaces some 1,500 m apart. Between two levels only \
    the mean is known, so a thin inversion shows as a slightly slower fall or not at all. A true \
    sounding needs the model's own levels.";

/// What the wind near by is and is not.
fn nearby_note(nearby: &ForecastNearby) -> String {
    format!(
        "Near by is the strongest wind and gust 10 m above the model's ground among the {} \
         squares of the forecast map within {:.0} km. A square is one cell of the model, so a \
         narrow valley's wind can be missing, and a square on a ridge has more wind than a \
         valley on any day. It is called much stronger from gusts of {:.0} km/h or a wind of \
         {:.0} km/h that is {:.0} km/h above the place's own, and it changes no verdict.",
        nearby.cells,
        nearby.radius_km,
        nearby::NEARBY_GUSTS_KMH,
        nearby::NEARBY_WIND_KMH,
        nearby::MUCH_STRONGER_KMH,
    )
}

/// The wind around the place, as the server gave it.
struct Around<'a> {
    /// The place the forecast is for.
    place: GeoPoint,
    nearby: &'a ForecastNearby,
}

impl Around<'_> {
    /// What the neighbourhood has in the hour the place's `hour` is of.
    fn at(&self, hour: &ForecastHour) -> Option<OutlookNearby> {
        let near = self
            .nearby
            .hours
            .iter()
            .find(|near| near.time == hour.time)?;
        Some(outlook_nearby(self.place, hour, near))
    }
}

/// Where a square lies from the place, in words: "25 km to the south", to
/// the nearest five kilometres, which is as fine as squares 11 km apart
/// allow. "At the place" within 2.5 km.
fn whereabouts_in_words(place: GeoPoint, square: GeoPoint) -> String {
    const POINTS: [&str; 8] = [
        "north",
        "north-east",
        "east",
        "south-east",
        "south",
        "south-west",
        "west",
        "north-west",
    ];
    let found = nearby::whereabouts(place, square);
    let km = (found.distance_km / 5.0).round() * 5.0;
    if km < 5.0 {
        return "in the square around the place".to_owned();
    }
    let point = POINTS[((found.bearing_deg.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8];
    format!("{km:.0} km to the {point}")
}

/// An hour of the neighbourhood beside the place's own hour.
fn outlook_nearby(place: GeoPoint, hour: &ForecastHour, near: &NearbyHour) -> OutlookNearby {
    let kmh = |ms: f64| ms * 3.6;
    let here = (hour.wind_speed_ms.map(kmh), hour.wind_gust_ms.map(kmh));
    let speed = |found: &Option<NearbyStrongest>| found.as_ref().map(|found| f64::from(found.kmh));
    let stronger = nearby::much_stronger(here, (speed(&near.wind), speed(&near.gust)));
    let place_of = |found: &NearbyStrongest| {
        let over = found
            .ground_m
            .map(|ground| format!(" over ground at {ground} m"))
            .unwrap_or_default();
        format!("{}{over}", whereabouts_in_words(place, found.at))
    };
    let line = match (stronger, &near.gust, &near.wind) {
        (Some(Stronger::Gusts), Some(gust), _) => format!(
            "Gusts to {} km/h {}, {:.0} km/h at the place",
            gust.kmh,
            place_of(gust),
            here.1.unwrap_or_default()
        ),
        (Some(Stronger::Wind), _, Some(wind)) => format!(
            "Wind {} km/h {}, {:.0} km/h at the place",
            wind.kmh,
            place_of(wind),
            here.0.unwrap_or_default()
        ),
        _ => String::new(),
    };
    let text = |found: &Option<NearbyStrongest>| {
        found
            .as_ref()
            .map(|found| found.kmh.to_string())
            .unwrap_or_default()
    };
    let position = |found: &Option<NearbyStrongest>| {
        found
            .as_ref()
            .map(|found| whereabouts_in_words(place, found.at))
            .unwrap_or_default()
    };
    let ground = |found: &Option<NearbyStrongest>| {
        found
            .as_ref()
            .and_then(|found| found.ground_m)
            .map(f64::from)
    };
    OutlookNearby {
        wind_kmh: text(&near.wind),
        wind_where: position(&near.wind),
        wind_ground_m: ground(&near.wind),
        gust_kmh: text(&near.gust),
        gust_where: position(&near.gust),
        gust_ground_m: ground(&near.gust),
        stronger: stronger.is_some(),
        line,
    }
}

/// The day's line about the wind near by: the hour in which what is much
/// stronger is strongest, gusts before a mean wind, the earliest of equal
/// ones. Empty when no hour's is much stronger.
fn nearby_of_day(hours: &[Judged<'_>]) -> String {
    let mut found: Option<(&Judged<'_>, (bool, u32))> = None;
    for judged in hours {
        let Some(near) = judged.nearby.as_ref().filter(|near| near.stronger) else {
            continue;
        };
        let gusts = near.line.starts_with("Gusts");
        let figure = if gusts {
            &near.gust_kmh
        } else {
            &near.wind_kmh
        };
        let weight = (gusts, figure.parse::<u32>().unwrap_or_default());
        if found.is_none_or(|(_, most)| weight > most) {
            found = Some((judged, weight));
        }
    }
    found
        .and_then(|(judged, _)| {
            let line = &judged.nearby.as_ref()?.line;
            let mut letters = line.chars();
            let first = letters.next()?.to_lowercase();
            Some(format!(
                "Much stronger near by at {}: {first}{}",
                clock(judged.local_hour),
                letters.as_str()
            ))
        })
        .unwrap_or_default()
}

/// The layers of an hour over ground at `ground_m`, as the outlook gives
/// them: those that begin below [`WINDS_UP_TO_M`], in words.
fn layers(hour: &ForecastHour, ground_m: f64) -> Vec<OutlookLayer> {
    layering::layers(hour, ground_m)
        .into_iter()
        .filter(|layer| layer.bottom_m < WINDS_UP_TO_M)
        .map(|layer| {
            let (bottom_m, top_m) = (fifty(layer.bottom_m), fifty(layer.top_m));
            let fall = tenth(layer.fall_c_per_100m);
            let inversion = layer.kind == Stability::Inversion;
            let words = if !inversion {
                format!(
                    "Stable layer between {bottom_m:.0} and {top_m:.0} m: {:.1} °C per 100 m",
                    fall.abs()
                )
            } else if fall == 0.0 {
                format!(
                    "Inversion between {bottom_m:.0} and {top_m:.0} m: barely warmer with height"
                )
            } else {
                format!(
                    "Inversion between {bottom_m:.0} and {top_m:.0} m: {:.1} °C warmer per 100 m",
                    -fall
                )
            };
            OutlookLayer {
                bottom_m,
                top_m,
                inversion,
                fall_c_per_100m: fall,
                words,
            }
        })
        .collect()
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
    /// The wind around the place in the hour, where the server gave it.
    nearby: Option<OutlookNearby>,
}

impl Judged<'_> {
    /// The hour as the shared rating of a day takes it.
    fn rated(&self) -> RatedHour {
        RatedHour {
            time: self.hour.time,
            verdict: self.verdict.into(),
            // The take-off's reasons are kept in words, beside.
            against: None,
            air: self.air,
            kind: self.thermals.map(Kind::from),
        }
    }
}

/// The hours as the shared rating of a day takes them.
fn rated(hours: &[Judged<'_>]) -> Vec<RatedHour> {
    hours.iter().map(Judged::rated).collect()
}

pub(crate) use venti_flight::day::{fifty, tenth};

fn days(
    series: &ForecastSeries,
    site: &Site<'_>,
    around: Option<&Around<'_>>,
    utc_offset_s: i32,
    now_unix_s: i64,
) -> Vec<OutlookDay> {
    let offset = Duration::seconds(i64::from(utc_offset_s));
    // The current hour is still to come for as long as it lasts.
    let from = now_unix_s - now_unix_s.rem_euclid(3600);
    // The first moment of the day that holds a moment, on the place's
    // clock.
    let start_of = |time: DateTime<Utc>| {
        (time + offset)
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map_or(time, |midnight| midnight.and_utc() - offset)
    };
    // Whether an hour lies in the daylight of the day that holds it, an
    // hour in from either end: the first and the last hour of the sun are
    // not for a judgement by wind and cloud alone.
    let in_daylight = |time: DateTime<Utc>| {
        let light = sun::daylight_at(start_of(time) + Duration::hours(12), series.cell);
        matches!(
            (light.sunrise, light.sunset),
            (Some(rise), Some(set))
                if time >= rise + Duration::hours(1) && time + Duration::hours(1) <= set
        )
    };
    // The day the model's run ends in with daylight left: the hour after
    // its last would be one of that day's daylight hours. Such a day is
    // judged by a part of its hours, and says where they end.
    let cut_short = series
        .hours
        .last()
        .map(|last| last.time + Duration::hours(1))
        .filter(|next| in_daylight(*next))
        .map(start_of);
    // Each day with its hours still to come, and whether the model had
    // daylight hours for it that are gone.
    let mut days: Vec<(DateTime<Utc>, Vec<Judged<'_>>, bool)> = Vec::new();
    for hour in &series.hours {
        let local = hour.time + offset;
        let start = start_of(hour.time);
        if days.last().is_none_or(|(day, _, _)| *day != start) {
            days.push((start, Vec::new(), false));
        }
        if !in_daylight(hour.time) {
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
            .and_then(|(air, launch)| kind_of_hour(&air, launch, verdict.into()))
            .map(Thermals::from);
        if let Some((_, hours, _)) = days.last_mut() {
            hours.push(Judged {
                hour,
                local_hour: local.hour(),
                verdict,
                why,
                air,
                thermals,
                nearby: around.and_then(|around| around.at(hour)),
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
                // Where the model's hours of the day end, if that is
                // before its daylight does.
                let ends = (cut_short == Some(start))
                    .then(|| hours.last().map(|judged| judged.local_hour + 1))
                    .flatten();
                let mut day = day(start.timestamp(), name, &hours, ground, ends.is_some());
                day.model = crate::forecast::model_name(series.model.as_str());
                day.run_unix_s = series.run.timestamp();
                day.note = note(series, day.thermals.is_some());
                if let Some(ends) = ends {
                    day.note += &format!(
                        " The model's run ends at {} on this day: the hours after are not judged.",
                        clock(ends)
                    );
                }
                day
            })
        })
        .collect()
}

/// What the forecast says about one hour: the worst thing against it. By
/// the air alone that is the shared rating's; the take-off adds the
/// directions it works in.
fn judge(hour: &ForecastHour, site: &Site<'_>) -> (Flyability, String) {
    let found = against(hour, site.elevation_m);
    let rain = matches!(found.first(), Some((_, Against::Rain)));
    // Where the wind's direction is weighed: after the wind itself.
    let after_wind = usize::from(matches!(found.first(), Some((_, Against::Wind { .. }))));
    let mut said: Vec<(Verdict, String)> = found
        .iter()
        .map(|(verdict, what)| (*verdict, words_against(what)))
        .collect();
    // A wind from where the take-off does not work, unless it is light:
    // then thermals and the slope decide, not the model.
    if !rain
        && let Some(speed) = hour.wind_speed_ms.map(|ms| ms * 3.6)
        && let Some(from) = hour.wind_direction_deg
        && wind_fits(site.wind.to_vec(), from) == Some(false)
    {
        let verdict = if speed >= WRONG_WAY_KMH.1 {
            Some(Verdict::NotFlyable)
        } else if speed >= WRONG_WAY_KMH.0 {
            Some(Verdict::Marginal)
        } else {
            None
        };
        if let Some(verdict) = verdict {
            said.insert(
                after_wind,
                (
                    verdict,
                    format!(
                        "Wind from {}, take-off works in {}",
                        compass(from),
                        site.works_in
                    ),
                ),
            );
        }
    }
    // The worst, and of two equally bad the one found first.
    let (verdict, why) = worst(&said);
    (verdict.into(), why.unwrap_or_default())
}

/// One thing the forecast has against an hour, in words.
fn words_against(what: &Against) -> String {
    match *what {
        Against::Rain => "Rain".to_owned(),
        Against::Wind { kmh } => format!("Wind {kmh:.0} km/h"),
        Against::Gusts { kmh } => format!("Gusts to {kmh:.0} km/h"),
        Against::WindAloft { kmh, at_m } => {
            let at = (at_m / 100.0).round() * 100.0;
            format!("Wind {kmh:.0} km/h at {at:.0} m")
        }
        Against::CloudBaseBelow => "Cloud base below the take-off".to_owned(),
        Against::CloudBaseNear => "Cloud base near the take-off".to_owned(),
        Against::Storms => "Showers or thunderstorms can grow".to_owned(),
    }
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
    let day = rating::thermal_day(&rated(hours))?;
    let climb_ms = tenth(day.climb_ms);
    let top_m = day.top_m.map(fifty);
    let line = match (day.window, top_m) {
        (Some((from, to)), Some(top_m)) => {
            let to_where = if day.cloud {
                format!("to a cloud base of {top_m:.0} m")
            } else {
                format!("to {top_m:.0} m in the blue")
            };
            format!(
                "{climb_ms:.1} m/s {to_where}, from {} to {}",
                clock(hours[from].local_hour),
                clock(hours[to].local_hour + 1)
            )
        }
        (None, Some(top_m)) if climb_ms > 0.0 => {
            format!("{climb_ms:.1} m/s to {top_m:.0} m at best")
        }
        _ => "None to climb in".to_owned(),
    };
    Some(ThermalDay {
        kind: day.kind.into(),
        line,
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

/// A day from its hours still to come. `cut_short` says that the model's
/// run ends after the last of them with daylight left: a stretch that
/// lasts to that hour is then not said to last to the end of the day.
fn day(
    start_unix_s: i64,
    name: String,
    hours: &[Judged<'_>],
    ground_m: Option<f64>,
    cut_short: bool,
) -> OutlookDay {
    let rated = rated(hours);
    let good = rating::stretch(&rated, Verdict::Flyable);
    let fair = rating::stretch(&rated, Verdict::Marginal);
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
            // To the end of the day: to its last hour, unless the model's
            // run ends there and the day's light goes on.
            let to_the_end = to == hours.len() - 1 && !cut_short;
            let line = match (from == 0, to_the_end) {
                (true, true) => "Good all day".to_owned(),
                (true, false) => format!("Good until {}", clock(ends)),
                (false, true) => format!("Good from {}", clock(begins)),
                (false, false) => format!("Good from {} to {}", clock(begins), clock(ends)),
            };
            // What ends the stretch, else what delays it.
            let why = if let Some(after) = hours.get(to + 1) {
                format!("{} after {}", after.why, clock(ends))
            } else if cut_short {
                format!("The forecast ends at {}", clock(ends))
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
        nearby: nearby_of_day(hours),
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
                layers: ground_m
                    .map(|ground| layers(judged.hour, ground))
                    .unwrap_or_default(),
                nearby: judged.nearby.clone(),
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
    /// clock from 8 to 19, each as the test wants it. After them stands
    /// the midnight that ends the day, with no values: the fine model's
    /// run goes on into the night, as a run that has a whole day does.
    fn answer_with(hours: &[(u32, &str)]) -> Vec<u8> {
        let mut hours: Vec<String> = hours
            .iter()
            .map(|(local, values)| {
                format!(
                    r#"{{"time": "2026-07-04T{:02}:00:00Z"{}{values}}}"#,
                    local - 2,
                    if values.is_empty() { "" } else { ", " }
                )
            })
            .collect();
        hours.push(r#"{"time": "2026-07-04T22:00:00Z"}"#.to_owned());
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
        // run ends. ICON-EU's run of 00 UTC reaches 78 hours, to 06 UTC of
        // the 7th: it has three days whole, and of the fourth the two
        // hours of daylight before eight in the morning.
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
        let coarse = [
            hours(4, 8..=19),
            hours(5, 8..=19),
            hours(6, 8..=19),
            hours(7, 7..=8),
        ]
        .join(",");
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
        // days after, of which it has none, are the coarse model's.
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
                ("Mon", "ICON-EU", 12),
                ("Tue", "ICON-EU", 2)
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

        // The fourth day is the one the coarse model's run ends in, with
        // most of its daylight still to come. It is judged by the two
        // hours there are, both fine; its line does not call that all day,
        // and its line and the foot of the screen say where the hours end.
        let last = &outlook.days[3];
        assert_eq!(last.verdict, Flyability::Flyable);
        assert_eq!(last.headline, "Good until 09:00");
        assert_eq!(last.why, "The forecast ends at 09:00");
        assert_eq!(
            last.note,
            "Read from ICON-EU for its grid cell, whose ground lies at 900 m. It knows nothing of \
             the take-off itself; the take-off decides. The model's run ends at 09:00 on this \
             day: the hours after are not judged."
        );
        // A day the run has to its end says no such thing.
        assert_eq!(outlook.days[2].headline, "Good all day");
        assert_eq!(outlook.days[2].why, "");
        assert!(!outlook.days[2].note.contains("run ends"));
    }

    #[test]
    fn a_day_the_run_ends_in_is_judged_by_its_hours_and_says_where_they_end() {
        const RAIN: &str = r#""wind_speed_ms": 2.0, "precipitation_mm": 0.6"#;
        // One model, whose run ends at 12 UTC of the day: 14:00 on the
        // valley's clock, with six hours of daylight after it.
        let outlook_of = |hour: &dyn Fn(u32) -> &'static str| {
            let hours: Vec<String> = (8..=14)
                .map(|local| {
                    format!(
                        r#"{{"time": "2026-07-04T{:02}:00:00Z", {}}}"#,
                        local - 2,
                        hour(local)
                    )
                })
                .collect();
            let body = format!(
                r#"{{
                "point": {{"lat": 47.7, "lon": 11.8}},
                "fetched_at": "2026-07-04T05:40:00Z",
                "layers": [{{"source": "open-meteo", "series": [
                    {{"source": "open-meteo", "model": "icon-eu", "run": "2026-07-01T06:00:00Z",
                      "point": {{"lat": 47.7, "lon": 11.8}},
                      "cell": {{"lat": 47.6875, "lon": 11.8125}},
                      "cell_elevation_m": 900.0, "hours": [{}]}}
                ]}}],
                "attributions": []
            }}"#,
                hours.join(",")
            );
            site_outlook(
                200,
                body.into_bytes(),
                SUMMER_TIME_S,
                north(),
                Some(1550.0),
                MORNING_S,
            )
            .unwrap()
            .days
            .remove(0)
        };
        const ENDS: &str =
            " The model's run ends at 15:00 on this day: the hours after are not judged.";

        // Rain until eleven and fine from noon to the run's last hour: the
        // stretch has a beginning and, with the run, an end.
        let day = outlook_of(&|hour| if hour < 12 { RAIN } else { FINE });
        assert_eq!(day.hours.len(), 7);
        assert_eq!(day.headline, "Good from noon to 15:00");
        assert_eq!(day.why, "The forecast ends at 15:00");
        assert!(day.note.ends_with(ENDS), "{}", day.note);

        // A stretch that something ends before the run does is said as on
        // any day; the foot still says where the hours end.
        let day = outlook_of(&|hour| if hour < 12 { FINE } else { RAIN });
        assert_eq!(day.headline, "Good until noon");
        assert_eq!(day.why, "Rain after noon");
        assert!(day.note.ends_with(ENDS), "{}", day.note);

        // A morning of rain makes the day not flyable by the hours there
        // are. Nothing is said of the afternoon, and the foot says so.
        let day = outlook_of(&|_| RAIN);
        assert_eq!(day.verdict, Flyability::NotFlyable);
        assert_eq!(day.headline, "Not flyable");
        assert_eq!(day.why, "Rain");
        assert!(day.note.ends_with(ENDS), "{}", day.note);
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

    /// The answer with the wind around the place added to it.
    fn with_nearby(answer: Vec<u8>, hours: &str) -> Vec<u8> {
        let text = String::from_utf8(answer).unwrap();
        let nearby = format!(
            r#""nearby": {{"source": "open-meteo", "radius_km": 30.0, "cells": 34, "hours": [{hours}]}},
            "attributions""#
        );
        assert!(text.contains(r#""attributions""#));
        text.replacen(r#""attributions""#, &nearby, 1).into_bytes()
    }

    #[test]
    fn a_much_stronger_wind_near_by_is_said_with_the_day_and_changes_no_verdict() {
        let fine: Vec<(u32, &str)> = (8..=19).map(|hour| (hour, FINE)).collect();
        // At 14:00 gusts of 55 km/h 22 km to the south and a wind of 28
        // in the place's own square; at 15:00 gusts of 38, which is under
        // the limit; at 16:00 a wind of 32 km/h 15 km to the east.
        let near = r#"
            {"time": "2026-07-04T12:00:00Z", "model": "icon-d2", "run": "2026-07-04T03:00:00Z",
             "wind": {"kmh": 28, "at": {"lat": 47.7, "lon": 11.8}, "from_deg": 180, "ground_m": 812},
             "gust": {"kmh": 55, "at": {"lat": 47.5, "lon": 11.8}, "from_deg": 180, "ground_m": 2400}},
            {"time": "2026-07-04T13:00:00Z", "model": "icon-d2", "run": "2026-07-04T03:00:00Z",
             "wind": {"kmh": 20, "at": {"lat": 47.5, "lon": 11.8}},
             "gust": {"kmh": 38, "at": {"lat": 47.5, "lon": 11.8}}},
            {"time": "2026-07-04T14:00:00Z", "model": "icon-d2", "run": "2026-07-04T03:00:00Z",
             "wind": {"kmh": 32, "at": {"lat": 47.7, "lon": 12.0}},
             "gust": {"kmh": 36, "at": {"lat": 47.7, "lon": 12.0}}}"#;
        let read = |body: Vec<u8>| {
            site_outlook(200, body, SUMMER_TIME_S, north(), Some(1550.0), MORNING_S).unwrap()
        };
        let without = read(answer_with(&fine));
        let with = read(with_nearby(answer_with(&fine), near));

        // The day is what it was: the wind near by is a line beside it.
        let day = &with.days[0];
        assert_eq!(day.verdict, Flyability::Flyable);
        assert_eq!(day.headline, "Good all day");
        assert_eq!(day.why, "");
        assert_eq!(
            day.nearby,
            "Much stronger near by at 14:00: gusts to 55 km/h 20 km to the south over ground \
             at 2400 m, 18 km/h at the place"
        );
        let verdicts = |outlook: &SiteOutlook| -> Vec<(Flyability, String)> {
            outlook.days[0]
                .hours
                .iter()
                .map(|hour| (hour.verdict, hour.why.clone()))
                .collect()
        };
        assert_eq!(verdicts(&with), verdicts(&without));

        let at = |local: &str| {
            day.hours
                .iter()
                .find(|hour| hour.hour == local)
                .unwrap()
                .nearby
                .clone()
        };
        assert_eq!(
            at("14"),
            Some(OutlookNearby {
                wind_kmh: "28".to_owned(),
                wind_where: "in the square around the place".to_owned(),
                wind_ground_m: Some(812.0),
                gust_kmh: "55".to_owned(),
                gust_where: "20 km to the south".to_owned(),
                gust_ground_m: Some(2400.0),
                stronger: true,
                line: "Gusts to 55 km/h 20 km to the south over ground at 2400 m, 18 km/h at the place"
                    .to_owned(),
            })
        );
        // Under the limit: the figures are there, and no line.
        let under = at("15").unwrap();
        assert_eq!((under.gust_kmh.as_str(), under.stronger), ("38", false));
        assert_eq!(under.line, "");
        assert_eq!(
            at("16").unwrap().line,
            "Wind 32 km/h 15 km to the east, 11 km/h at the place"
        );
        // An hour the server's map has nothing for.
        assert_eq!(at("9"), None);
        assert!(
            with.nearby_note.contains("among the 34 squares")
                && with.nearby_note.contains("within 30 km")
                && with
                    .nearby_note
                    .contains("gusts of 40 km/h or a wind of 30 km/h")
                && with.nearby_note.contains("15 km/h above"),
            "{}",
            with.nearby_note
        );

        // Without the neighbourhood, as from a server that makes no map.
        assert_eq!(without.days[0].nearby, "");
        assert!(
            without.days[0]
                .hours
                .iter()
                .all(|hour| hour.nearby.is_none())
        );
        assert_eq!(without.nearby_note, "");
    }

    #[test]
    fn where_the_temperature_falls_too_slowly_or_rises_an_hour_has_a_layer() {
        // Ground at 812 m. In the morning the air at 180 m above it is
        // warmer than at the ground; all day the air between the surfaces
        // at 1500 and 3100 m cools by 4.8 °C, 0.3 °C per 100 m, and
        // above 5700 m it hardly cools at all.
        let levels = |ground_c: f64| {
            format!(
                r#""wind_speed_ms": 3.0, "wind_direction_deg": 0.0, "temperature_c": {ground_c:.1},
                "winds_aloft": [
                {{"above_ground_m": 80.0, "temperature_c": 17.0}},
                {{"above_ground_m": 180.0, "temperature_c": 18.5}},
                {{"pressure_hpa": 850.0, "altitude_m": 1500.0, "temperature_c": 14.0}},
                {{"pressure_hpa": 700.0, "altitude_m": 3100.0, "temperature_c": 9.2}},
                {{"pressure_hpa": 600.0, "altitude_m": 4300.0, "temperature_c": 0.0}},
                {{"pressure_hpa": 500.0, "altitude_m": 5700.0, "temperature_c": -10.0}},
                {{"pressure_hpa": 400.0, "altitude_m": 7200.0, "temperature_c": -11.0}}]"#
            )
        };
        let outlook = outlook_of(|hour| levels(if hour < 10 { 15.0 } else { 26.0 }));
        let day = &outlook.days[0];

        let morning: Vec<&str> = day.hours[0]
            .layers
            .iter()
            .map(|layer| layer.words.as_str())
            .collect();
        assert_eq!(
            morning,
            [
                "Inversion between 800 and 1000 m: 2.0 °C warmer per 100 m",
                "Stable layer between 1500 and 3100 m: 0.3 °C per 100 m",
            ]
        );
        assert!(day.hours[0].layers[0].inversion);
        assert_eq!(day.hours[0].layers[0].fall_c_per_100m, -2.0);

        // By noon the ground has warmed and the lowest step is gone; the
        // layer higher up stays. The stable air from 5700 m up begins
        // above where a paraglider flies and is not listed.
        let noon = &day.hours[4].layers;
        assert_eq!(
            noon.iter()
                .map(|layer| layer.words.as_str())
                .collect::<Vec<_>>(),
            ["Stable layer between 1500 and 3100 m: 0.3 °C per 100 m"]
        );
        assert_eq!(
            (
                noon[0].bottom_m,
                noon[0].top_m,
                noon[0].inversion,
                noon[0].fall_c_per_100m
            ),
            (1500.0, 3100.0, false, 0.3)
        );
        assert!(outlook.layers_note.contains("true sounding"));

        // An hour without temperatures higher up has none, and neither
        // has air that cools fast enough all the way.
        let bare = self::outlook(&[(12, FINE), (13, FINE)]);
        assert!(bare.days[0].hours.iter().all(|hour| hour.layers.is_empty()));
        let unstable = outlook_of(|_| air(400.0, 3400.0, 10.0, 4.0));
        assert!(
            unstable.days[0]
                .hours
                .iter()
                .all(|hour| hour.layers.is_empty())
        );
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
