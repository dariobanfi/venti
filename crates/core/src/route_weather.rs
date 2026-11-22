//! A planned route with the weather along it, and the height to leave
//! each point at.
//!
//! The pilot draws a route ([`crate::plan_route`]), names a start time, a
//! mean speed over the ground and a glide ratio. The shell asks the server
//! for the thermal map's hours at the route's points
//! ([`route_weather_path`], `GET /v1/route/weather`), reads the answer once
//! ([`RouteForecast::read`]) and hands it to [`route_along`] with the
//! terrain it holds. What comes back says, point by point and leg by leg,
//! when the pilot would be there and what the forecast has there then, in
//! words, and one line for the whole route. Without a forecast, or without
//! the ground, it says what it can: the times, and the heights in still
//! air.
//!
//! **The rules.** Fixed, ours, and the owner's to change:
//!
//! - **When.** The pilot is at a point after the distance along the legs
//!   to it at the mean speed over the ground
//!   ([`venti_flight::route_plan::times_at_points`]), a speed with the
//!   climbs in it, held between [`SPEED_KMH`]. The forecast's hour read
//!   for a point is the one nearest that moment, if it lies within half an
//!   hour of it; otherwise the point has none ("outside the forecast's
//!   hours").
//! - **Where.** A point is read in the map's square it lies in, as the
//!   server answers it. A point of the answer belongs to a place of the
//!   route only if it lies within [`SAME_PLACE_M`] of it: a route changed
//!   since the forecast was asked keeps its new places without one.
//! - **At which height.** The wind is read at the height the pilot set
//!   for the route; without one, at the thermals' top there and then; in
//!   an hour without thermals, [`ABOVE_GROUND_M`] over the model's ground.
//!   It is read between the heights the map keeps and the wind 10 m over
//!   the ground as [`venti_flight::aloft::between_levels`] reads it, and
//!   not beyond them: above 4000 m or under the ground there is none.
//! - **The wind on a leg.** The mean, as arrows, of the wind at its two
//!   ends at the times the pilot is there, split along and across the leg
//!   ([`venti_flight::route_plan`]). Under [`CALM_KMH`] it is "little
//!   wind"; otherwise the larger of the two parts names it: a head or a
//!   tail wind, or a cross wind from the left or the right.
//! - **Rain on a leg.** An hour of either end's square from the hour
//!   nearest leaving to the hour nearest arriving with at least the flight
//!   engine's 0.2 mm (`venti_flight::day::RAIN_MM`): the leg "runs into
//!   rain" by the first such hour. An hour's rain is the hour before it,
//!   as the model gives it.
//! - **Cloud.** From [`OVERCAST_PCT`] of the sky at a point, "overcast".
//! - **The height to leave at.** To reach the next point in a straight
//!   glide with [`MARGIN_M`] over the ground everywhere on the line, at the
//!   glider's trim speed (`Glider::default`, 38 km/h) with the pilot's
//!   glide ratio, in the leg's wind
//!   ([`venti_flight::route_plan::departure_height`]), rounded up to ten
//!   metres. The air neither rises nor sinks on the way, nothing flies
//!   round a ridge, and the ground is the terrain the shell loaded: a line
//!   whose ground is not all loaded says so.
//! - **The whole route.** How long it takes and from when to when; the
//!   first leg that runs into rain; every point whose hour the forecast
//!   rules out; the strongest head wind of
//!   [`HEAD_WIND_WORTH_SAYING_KMH`] or more; and whether the route ends
//!   after the forecast's last daylight hour there.
//!
//! It is a reading of a weather model for squares some ten kilometres
//! wide, at the hours a mean speed puts the pilot there. A valley's own
//! wind, a lee, a climb that takes longer than the mean: none of it is in
//! it.

use std::sync::Arc;

use chrono::DateTime;
use venti_api_types::v1;
use venti_domain::GeoPoint;
use venti_flight::aloft::{WindAt, between_levels};
use venti_flight::day::RAIN_MM;
use venti_flight::live::Wind;
use venti_flight::live::navigation::{Glider, bearing_deg};
use venti_flight::route_plan::{
    Departure, departure_height, mean_wind, times_at_points, wind_on_course,
};

use crate::forecast::model_name;
use crate::server::answer;
use crate::sites::compass;
use crate::terrain::Terrain;
use crate::wind::credit;
use crate::{CoreError, Credit, Flyability, Thermals};

/// The mean speed over the ground is held between these, km/h: a walk
/// and a fast glider on a racing day.
pub const SPEED_KMH: (f64, f64) = (5.0, 80.0);

/// A glide ratio is held between these.
pub const GLIDE_RATIO: (f64, f64) = (2.0, 20.0);

/// What a departure height keeps over the ground on the way, metres.
pub const MARGIN_M: f64 = 200.0;

/// In an hour without thermals the wind is read this far over the
/// model's ground, metres.
pub const ABOVE_GROUND_M: f64 = 500.0;

/// Under this a leg's wind is "little wind", km/h.
pub const CALM_KMH: f64 = 5.0;

/// From this share of the sky a point is "overcast", percent.
pub const OVERCAST_PCT: u8 = 90;

/// A head wind from this strength is named in the route's line, km/h.
pub const HEAD_WIND_WORTH_SAYING_KMH: f64 = 10.0;

/// A point of the server's answer belongs to a place of the route within
/// this distance, metres.
pub const SAME_PLACE_M: f64 = 100.0;

/// The forecast's hour is read for a moment within this of it, seconds.
const HOUR_WITHIN_S: i64 = 1800;

/// A place of a planned route.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RoutePlace {
    /// Latitude, WGS84 degrees.
    pub lat: f64,
    /// Longitude, WGS84 degrees.
    pub lon: f64,
    /// Its name, if it has one: a waypoint's or a site's. Without one the
    /// words call it by its number: "point 2".
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", serde(default))]
    pub name: Option<String>,
}

/// How the pilot means to fly a route.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RouteSetting {
    /// The places in the order they are flown.
    pub places: Vec<RoutePlace>,
    /// When the pilot leaves the first, Unix seconds.
    pub start_unix_s: i64,
    /// The pilot's mean speed over the ground, climbs included, km/h.
    pub ground_speed_kmh: f64,
    /// The glider's glide ratio in still air.
    pub glide_ratio: f64,
    /// The height the pilot means to fly at, metres above mean sea level.
    /// Without it the wind is read at the thermals' top.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", serde(default))]
    pub flying_height_m: Option<f64>,
}

/// One place of the route, with what the forecast has there when the
/// pilot would be there, and the height to leave it at.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RoutePointWeather {
    /// Its name, or "Point 2".
    pub name: String,
    /// When the pilot would be there, Unix seconds.
    pub at_unix_s: i64,
    /// That moment on the pilot's clock: "14:10".
    pub clock: String,
    /// The forecast's hour read for it, Unix seconds. Absent without a
    /// forecast there then.
    pub hour_unix_s: Option<i64>,
    /// Whether the forecast's hour can be flown by the air alone.
    pub flyability: Option<Flyability>,
    /// What its thermals count as.
    pub thermals: Option<Thermals>,
    /// The climb to expect, m/s.
    pub climb_ms: Option<f64>,
    /// How high the thermals go, metres above mean sea level.
    pub thermal_top_m: Option<i32>,
    /// The height the wind is read at, metres above mean sea level.
    pub wind_height_m: Option<f64>,
    /// The wind there, km/h.
    pub wind_kmh: Option<f64>,
    /// Where it comes from, degrees.
    pub wind_from_deg: Option<f64>,
    /// The share of the sky covered by cloud, percent.
    pub cloud_cover_pct: Option<u8>,
    /// The rain of the hour, millimetres.
    pub rain_mm: Option<f64>,
    /// The height to leave it at for the next point, metres above mean sea
    /// level. Absent at the last point, and where none can be said.
    pub departure_m: Option<f64>,
    /// The height in words: "Leave Brauneck at 1850 m to reach Wallberg
    /// with 200 m to spare in this wind." Empty at the last point.
    pub departure_words: String,
    /// The forecast there in words: "Wallberg at 14:10: local thermals to
    /// 1900 m, 1.2 m/s. 12 km/h from the W at 1900 m. Cloud 40%."
    pub words: String,
}

/// One leg of the route, with its wind.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RouteLegWeather {
    /// The leg's length, metres.
    pub distance_m: f64,
    /// The direction to fly it, degrees.
    pub bearing_deg: f64,
    /// The part of the leg's wind against the glider, km/h: positive a
    /// head wind, negative a tail wind. Absent without a wind.
    pub head_wind_kmh: Option<f64>,
    /// The part across it, km/h: positive from the left. Absent without a
    /// wind.
    pub cross_wind_kmh: Option<f64>,
    /// The first hour on the leg with rain, Unix seconds.
    pub rain_from_unix_s: Option<i64>,
    /// In words: "The second leg, 18.4 km to Hochfelln from 14:10 to
    /// 14:55: 12 km/h head wind. It runs into rain by 15:00."
    pub words: String,
}

/// A planned route with the weather along it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RouteAlong {
    /// The places, in order.
    pub points: Vec<RoutePointWeather>,
    /// The legs, one fewer than the places.
    pub legs: Vec<RouteLegWeather>,
    /// When the pilot would be at the last place, Unix seconds.
    pub finish_unix_s: i64,
    /// The whole route in a few sentences: "The route takes 2 h 10 at
    /// 25 km/h, from 11:00 to 13:10. The second leg runs into rain by
    /// 13:00."
    pub words: String,
    /// The forecast's model and run in words: "ICON-D2, run of 06:00".
    /// Absent without a forecast.
    pub model: Option<String>,
    /// The credit to show with it.
    pub credits: Vec<Credit>,
}

/// The path and query to `GET` from the server for the forecast along a
/// route: its places and the day, as the thermal map names it
/// ([`crate::ThermalMapDay::date`]). At most 25 places.
#[uniffi::export]
pub fn route_weather_path(places: Vec<RoutePlace>, date: String) -> Result<String, CoreError> {
    let refused = || CoreError::Server {
        message: "The app could not ask the server for the weather along this route.".to_owned(),
    };
    let date = v1::parse_day(&date).map_err(|_| refused())?;
    let points: Vec<GeoPoint> = places
        .iter()
        .map(|place| GeoPoint::new(place.lat, place.lon))
        .collect::<Result<_, _>>()
        .map_err(|_| refused())?;
    if points.is_empty() || points.len() > v1::ROUTE_POINTS_AT_MOST {
        return Err(refused());
    }
    Ok(v1::RouteWeatherRequest { points, date }.path_and_query())
}

/// The server's answer of the forecast along a route, read once.
#[derive(Debug, uniffi::Object)]
pub struct RouteForecast {
    response: v1::RouteWeatherResponse,
}

#[uniffi::export]
impl RouteForecast {
    /// Reads the server's answer to [`route_weather_path`].
    #[uniffi::constructor]
    pub fn read(status: u16, body: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let response: v1::RouteWeatherResponse = answer(status, &body)?;
        Ok(Arc::new(Self { response }))
    }
}

/// A route as the pilot means to fly it, with the forecast along it from
/// the server's answer and the heights to leave each point at over the
/// terrain the shell loaded. Places that are not on the globe are left
/// out. `utc_offset_s` is how far the pilot's clock is ahead of UTC.
#[uniffi::export]
pub fn route_along(
    setting: RouteSetting,
    forecast: Option<Arc<RouteForecast>>,
    terrain: Option<Arc<Terrain>>,
    utc_offset_s: i32,
) -> RouteAlong {
    let places: Vec<(GeoPoint, String)> = setting
        .places
        .iter()
        .filter_map(|place| {
            Some((
                GeoPoint::new(place.lat, place.lon).ok()?,
                place.name.clone(),
            ))
        })
        .enumerate()
        .map(|(index, (point, name))| {
            let name = name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| format!("Point {}", index + 1));
            (point, name)
        })
        .collect();
    let speed_kmh = clamped(setting.ground_speed_kmh, SPEED_KMH);
    let glider = Glider {
        glide_ratio: clamped(setting.glide_ratio, GLIDE_RATIO),
        ..Glider::default()
    };
    let legs_m: Vec<f64> = places
        .windows(2)
        .map(|pair| pair[0].0.distance_m(pair[1].0))
        .collect();
    let times: Vec<i64> = times_at_points(&legs_m, speed_kmh / 3.6)
        .unwrap_or_default()
        .iter()
        .map(|seconds| setting.start_unix_s + whole(*seconds))
        .collect();
    let response = forecast.as_ref().map(|forecast| &forecast.response);
    let clock = |unix_s: i64| clock(unix_s, utc_offset_s);

    // What the forecast has at each place, at its time.
    let read: Vec<Reading> = places
        .iter()
        .enumerate()
        .map(|(index, (point, _))| {
            let at = response.and_then(|response| {
                response
                    .points
                    .get(index)
                    .filter(|of| of.at.distance_m(*point) <= SAME_PLACE_M)
            });
            Reading::of(at, times[index], setting.flying_height_m)
        })
        .collect();

    let mut legs = Vec::with_capacity(legs_m.len());
    let mut departures = vec![(None, String::new()); places.len()];
    for (leg, pair) in places.windows(2).enumerate() {
        let ((from, from_name), (to, to_name)) = (&pair[0], &pair[1]);
        let bearing = bearing_deg(*from, *to);
        let wind = match (read[leg].wind, read[leg + 1].wind) {
            (Some(one), Some(other)) => Some(mean_wind(one, other)),
            (one, other) => one.or(other),
        };
        let parts = wind.map(|wind| wind_on_course(bearing, wind));
        let rain_from = rain_on_leg(&read[leg], &read[leg + 1], times[leg], times[leg + 1]);
        let mut words = format!(
            "{}, {:.1} km to {to_name} from {} to {}: {}.",
            leg_name(leg),
            legs_m[leg] / 1000.0,
            clock(times[leg]),
            clock(times[leg + 1]),
            wind.map_or_else(
                || "no wind from the forecast".to_owned(),
                |wind| { wind_words(wind, bearing) }
            )
        );
        if let Some(rain) = rain_from {
            words.push_str(&format!(" It runs into rain by {}.", clock(rain)));
        }
        legs.push(RouteLegWeather {
            distance_m: legs_m[leg],
            bearing_deg: bearing,
            head_wind_kmh: parts.map(|parts| -parts.along_ms * 3.6),
            cross_wind_kmh: parts.map(|parts| parts.across_ms * 3.6),
            rain_from_unix_s: rain_from,
            words,
        });
        departures[leg] = departure(
            terrain.as_deref(),
            (*from, from_name),
            (*to, to_name),
            wind,
            &glider,
            read[leg].top_m,
        );
    }

    let points: Vec<RoutePointWeather> = places
        .iter()
        .zip(&read)
        .zip(departures)
        .zip(&times)
        .map(
            |((((_, name), reading), (departure_m, departure_words)), at)| RoutePointWeather {
                name: capitalised(name),
                at_unix_s: *at,
                clock: clock(*at),
                hour_unix_s: reading.hour.map(|hour| hour.time.timestamp()),
                flyability: reading.hour.and_then(|hour| hour.verdict).map(flyability),
                thermals: reading.hour.and_then(|hour| hour.thermals).map(thermals),
                climb_ms: reading.hour.and_then(|hour| hour.climb_ms),
                thermal_top_m: reading.top_m,
                wind_height_m: reading.wind_height_m,
                wind_kmh: reading.wind.map(|wind| wind.speed_ms * 3.6),
                wind_from_deg: reading.wind.map(|wind| wind.from_deg),
                cloud_cover_pct: reading.hour.and_then(|hour| hour.cloud_cover_pct),
                rain_mm: reading.hour.and_then(|hour| hour.precipitation_mm),
                departure_m,
                departure_words,
                words: point_words(name, *at, reading, response.is_some(), &clock),
            },
        )
        .collect();

    let finish_unix_s = times.last().copied().unwrap_or(setting.start_unix_s);
    let words = route_words(
        &points,
        &legs,
        &read,
        (setting.start_unix_s, finish_unix_s),
        speed_kmh,
        response.is_some(),
        &clock,
    );
    RouteAlong {
        points,
        legs,
        finish_unix_s,
        words,
        model: response.map(|response| {
            format!(
                "{}, run of {}",
                model_name(response.model.as_str()),
                clock(response.run.timestamp())
            )
        }),
        credits: response
            .map(|response| response.attributions.iter().map(credit).collect())
            .unwrap_or_default(),
    }
}

/// What the forecast has at one place at the moment the pilot is there.
struct Reading<'a> {
    /// The place is in the forecast's map.
    in_map: bool,
    /// The hour read, if one lies near enough.
    hour: Option<&'a v1::RouteWeatherHour>,
    /// Every hour of the place, for the rain on a leg.
    hours: &'a [v1::RouteWeatherHour],
    top_m: Option<i32>,
    wind_height_m: Option<f64>,
    wind: Option<Wind>,
    /// After the place's last hour of the day.
    after_last: bool,
}

impl<'a> Reading<'a> {
    fn of(at: Option<&'a v1::RouteWeatherPoint>, unix_s: i64, height_m: Option<f64>) -> Self {
        let hours = at.map_or(&[][..], |at| at.hours.as_slice());
        let hour = hours
            .iter()
            .min_by_key(|hour| (hour.time.timestamp() - unix_s).abs())
            .filter(|hour| (hour.time.timestamp() - unix_s).abs() <= HOUR_WITHIN_S);
        let ground_m = at.and_then(|at| at.ground_m).map(f64::from);
        let top_m = hour.and_then(|hour| hour.top_m);
        // To ten metres: a height the words can say.
        let wind_height_m = hour.and_then(|_| {
            height_m
                .or(top_m.map(f64::from))
                .or(ground_m.map(|ground| ground + ABOVE_GROUND_M))
                .map(|height| (height / 10.0).round() * 10.0)
        });
        let wind = hour
            .zip(wind_height_m)
            .and_then(|(hour, height)| wind_at(hour, ground_m, height));
        Self {
            in_map: at.is_some_and(|at| at.cell.is_some()),
            hour,
            hours,
            top_m,
            wind_height_m,
            wind,
            after_last: hours
                .last()
                .is_some_and(|last| unix_s > last.time.timestamp() + HOUR_WITHIN_S),
        }
    }
}

/// The wind of an hour at a height, read between the wind over the
/// ground and the heights the map keeps.
fn wind_at(hour: &v1::RouteWeatherHour, ground_m: Option<f64>, height_m: f64) -> Option<Wind> {
    let at = |kmh: u16, from: Option<u16>| WindAt {
        speed_ms: f64::from(kmh) / 3.6,
        from_deg: from.map(f64::from),
    };
    let mut levels: Vec<(f64, WindAt)> = Vec::new();
    if let (Some(ground), Some(kmh)) = (ground_m, hour.wind_kmh) {
        levels.push((
            ground + venti_flight::aloft::LOWEST_WIND_M,
            at(kmh, hour.wind_from_deg),
        ));
    }
    for wind in &hour.aloft {
        let height = f64::from(wind.height_m);
        if ground_m.is_none_or(|ground| height > ground) {
            levels.push((height, at(wind.kmh, wind.from_deg)));
        }
    }
    levels.sort_by(|a, b| a.0.total_cmp(&b.0));
    let wind = between_levels(&levels, height_m)?;
    Some(Wind {
        from_deg: wind.from_deg?,
        speed_ms: wind.speed_ms,
    })
}

/// The first hour with rain at either end of a leg, from the hour nearest
/// leaving to the hour nearest arriving.
fn rain_on_leg(from: &Reading, to: &Reading, leave: i64, arrive: i64) -> Option<i64> {
    let (first, last) = (leave - HOUR_WITHIN_S, arrive + HOUR_WITHIN_S);
    from.hours
        .iter()
        .chain(to.hours)
        .filter(|hour| (first..=last).contains(&hour.time.timestamp()))
        .filter(|hour| hour.precipitation_mm.is_some_and(|rain| rain >= RAIN_MM))
        .map(|hour| hour.time.timestamp())
        .min()
}

/// The leg's wind in words: "12 km/h head wind".
fn wind_words(wind: Wind, bearing: f64) -> String {
    let kmh = wind.speed_ms * 3.6;
    if kmh < CALM_KMH {
        return format!("little wind, {kmh:.0} km/h");
    }
    let parts = wind_on_course(bearing, wind);
    let (along, across) = (parts.along_ms * 3.6, parts.across_ms * 3.6);
    if along.abs() >= across.abs() {
        let kind = if along < 0.0 { "head" } else { "tail" };
        format!("{:.0} km/h {kind} wind", along.abs())
    } else {
        let side = if across > 0.0 { "left" } else { "right" };
        format!("{:.0} km/h cross wind from the {side}", across.abs())
    }
}

/// "The first leg", up to the tenth, then "Leg 11".
fn leg_name(leg: usize) -> String {
    const ORDINALS: [&str; 10] = [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
        "tenth",
    ];
    ORDINALS.get(leg).map_or_else(
        || format!("Leg {}", leg + 1),
        |word| format!("The {word} leg"),
    )
}

/// The height to leave a place at, and in words.
fn departure(
    terrain: Option<&Terrain>,
    (from, from_name): (GeoPoint, &str),
    (to, to_name): (GeoPoint, &str),
    wind: Option<Wind>,
    glider: &Glider,
    top_m: Option<i32>,
) -> (Option<f64>, String) {
    let from_name = capitalised(from_name);
    let found = terrain.map_or(Departure::NoGround, |terrain| {
        terrain.with(|ground| departure_height(ground, from, to, wind, glider, MARGIN_M))
    });
    match found {
        Departure::NoWay => (
            None,
            format!("In this wind the glider makes no way from {from_name} to {to_name}."),
        ),
        Departure::NoGround => (
            None,
            format!(
                "The ground between {from_name} and {to_name} is not loaded, so there is no \
                 height to leave at yet."
            ),
        ),
        Departure::Height {
            altitude_m,
            ground_held,
            ..
        } => {
            let altitude_m = (altitude_m / 10.0).ceil() * 10.0;
            let mut words = format!(
                "Leave {from_name} at {altitude_m:.0} m to reach {to_name} with {MARGIN_M:.0} m \
                 to spare {}.",
                if wind.is_some() {
                    "in this wind"
                } else {
                    "in still air"
                }
            );
            if ground_held < 1.0 {
                words.push_str(" Part of the ground on the way is not loaded: it may need more.");
            }
            if let Some(top) = top_m.filter(|top| f64::from(*top) < altitude_m) {
                words.push_str(&format!(
                    " That is above the thermals there, which reach {top} m."
                ));
            }
            (Some(altitude_m), words)
        }
    }
}

/// The forecast at a place in words.
fn point_words(
    name: &str,
    at: i64,
    reading: &Reading,
    has_forecast: bool,
    clock: &dyn Fn(i64) -> String,
) -> String {
    let head = format!("{} at {}", capitalised(name), clock(at));
    if !has_forecast {
        return format!("{head}.");
    }
    if !reading.in_map {
        return format!("{head}: outside the forecast's map.");
    }
    let Some(hour) = reading.hour else {
        return format!("{head}: outside the forecast's hours there.");
    };
    let mut parts = Vec::new();
    match (hour.verdict, hour.thermals) {
        (Some(v1::ThermalVerdict::NotFlyable), _) => parts.push(format!(
            "not flyable by the forecast{}",
            hour.why
                .map_or_else(String::new, |why| format!(", {}", reason(why)))
        )),
        (_, Some(kind)) => {
            let mut said = thermal_words(kind).to_owned();
            if let Some(top) = hour.top_m {
                said.push_str(&format!(" to {top} m"));
            }
            if let Some(climb) = hour.climb_ms.filter(|climb| *climb >= 0.1) {
                said.push_str(&format!(", {climb:.1} m/s"));
            }
            parts.push(said);
        }
        _ => parts.push("no thermals in the forecast".to_owned()),
    }
    let mut words = format!("{head}: {}.", parts.join(", "));
    if let (Some(wind), Some(height)) = (reading.wind, reading.wind_height_m) {
        words.push_str(&format!(
            " {:.0} km/h from the {} at {height:.0} m.",
            wind.speed_ms * 3.6,
            compass(wind.from_deg)
        ));
    }
    if let Some(cloud) = hour.cloud_cover_pct {
        if cloud >= OVERCAST_PCT {
            words.push_str(" Overcast.");
        } else {
            words.push_str(&format!(" Cloud {cloud}%."));
        }
    }
    if let Some(rain) = hour.precipitation_mm.filter(|rain| *rain >= RAIN_MM) {
        words.push_str(&format!(" Rain, {rain:.1} mm in the hour."));
    }
    words
}

/// The whole route in a few sentences.
fn route_words(
    points: &[RoutePointWeather],
    legs: &[RouteLegWeather],
    read: &[Reading],
    (start, finish): (i64, i64),
    speed_kmh: f64,
    has_forecast: bool,
    clock: &dyn Fn(i64) -> String,
) -> String {
    if points.len() < 2 {
        return "Tap the map where the route goes.".to_owned();
    }
    let minutes = (finish - start) / 60;
    let took = if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{} h {:02}", minutes / 60, minutes % 60)
    };
    let mut words = format!(
        "The route takes {took} at {speed_kmh:.0} km/h, from {} to {}.",
        clock(start),
        clock(finish)
    );
    if !has_forecast {
        words.push_str(" No forecast along it: the heights to leave at are for still air.");
        return words;
    }
    let mut troubles = 0;
    let outside: Vec<&str> = points
        .iter()
        .zip(read)
        .filter(|(_, reading)| !reading.in_map)
        .map(|(point, _)| point.name.as_str())
        .collect();
    match outside.as_slice() {
        [] => {}
        [one] => words.push_str(&format!(" {one} lies outside the forecast's map.")),
        many => words.push_str(&format!(
            " {} lie outside the forecast's map.",
            listed(many)
        )),
    }
    if let Some((leg, rain)) = legs
        .iter()
        .enumerate()
        .find_map(|(leg, weather)| weather.rain_from_unix_s.map(|rain| (leg, rain)))
    {
        words.push_str(&format!(
            " {} runs into rain by {}.",
            leg_name(leg),
            clock(rain)
        ));
        troubles += 1;
    }
    for point in points
        .iter()
        .filter(|point| point.flyability == Some(Flyability::NotFlyable))
    {
        words.push_str(&format!(
            " At {} at {} the forecast rules the air out.",
            point.name, point.clock
        ));
        troubles += 1;
    }
    let strongest = legs
        .iter()
        .enumerate()
        .filter_map(|(leg, weather)| weather.head_wind_kmh.map(|head| (leg, head)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|(_, head)| *head >= HEAD_WIND_WORTH_SAYING_KMH);
    if let Some((leg, head)) = strongest {
        words.push_str(&format!(
            " The strongest head wind is on the leg to {}, {head:.0} km/h.",
            points[leg + 1].name
        ));
    }
    if read.last().is_some_and(|last| last.after_last) {
        words.push_str(" It ends after the forecast's last daylight hour there.");
        troubles += 1;
    }
    if troubles == 0 {
        words.push_str(" Nothing in the forecast rules out an hour on the way.");
    }
    words
}

/// "Brauneck, Wallberg and Hochfelln".
pub(crate) fn listed(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn thermal_words(kind: v1::ThermalKind) -> &'static str {
    match kind {
        v1::ThermalKind::NoThermals => "no thermals",
        v1::ThermalKind::Weak => "weak thermals",
        v1::ThermalKind::Local => "local thermals",
        v1::ThermalKind::CrossCountry => "cross-country thermals",
        v1::ThermalKind::Big => "big thermals",
    }
}

fn reason(why: v1::ThermalReason) -> &'static str {
    match why {
        v1::ThermalReason::Rain => "rain",
        v1::ThermalReason::Wind => "wind",
        v1::ThermalReason::Gusts => "gusts",
        v1::ThermalReason::WindAloft => "wind higher up",
        v1::ThermalReason::CloudBase => "a low cloud base",
        v1::ThermalReason::Storms => "showers or thunderstorms",
    }
}

fn flyability(verdict: v1::ThermalVerdict) -> Flyability {
    match verdict {
        v1::ThermalVerdict::Flyable => Flyability::Flyable,
        v1::ThermalVerdict::Marginal => Flyability::Marginal,
        v1::ThermalVerdict::NotFlyable => Flyability::NotFlyable,
    }
}

fn thermals(kind: v1::ThermalKind) -> Thermals {
    match kind {
        v1::ThermalKind::NoThermals => Thermals::NoThermals,
        v1::ThermalKind::Weak => Thermals::Weak,
        v1::ThermalKind::Local => Thermals::Local,
        v1::ThermalKind::CrossCountry => Thermals::CrossCountry,
        v1::ThermalKind::Big => Thermals::Big,
    }
}

/// A value held between two, and the lower for one that is no number.
fn clamped(value: f64, (low, high): (f64, f64)) -> f64 {
    if value.is_finite() {
        value.clamp(low, high)
    } else {
        low
    }
}

#[allow(clippy::cast_possible_truncation)]
fn whole(seconds: f64) -> i64 {
    seconds.round() as i64
}

/// "Point 2" at the head of a sentence.
fn capitalised(name: &str) -> String {
    let mut letters = name.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + letters.as_str())
        .unwrap_or_default()
}

/// A moment on the pilot's clock: "14:35".
fn clock(unix_s: i64, utc_offset_s: i32) -> String {
    DateTime::from_timestamp(unix_s + i64::from(utc_offset_s), 0)
        .map_or_else(String::new, |time| time.format("%H:%M").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The development mode's answer for the Wallberg, a place south-east
    /// of it and the Brauneck, on 22 November 2026.
    const ANSWER: &[u8] = include_bytes!("../../server/tests/golden/route_weather.json");

    fn place(lat: f64, lon: f64, name: &str) -> RoutePlace {
        RoutePlace {
            lat,
            lon,
            name: Some(name.to_owned()),
        }
    }

    fn places() -> Vec<RoutePlace> {
        vec![
            place(47.6642, 11.7967, "Wallberg"),
            place(47.62, 11.91, "Spitzing"),
            place(47.6603, 11.5197, "Brauneck"),
        ]
    }

    /// 22 November 2026, 09:00 UTC: 10:00 on a clock of Central Europe.
    const NINE_UTC: i64 = 1_795_338_000;

    fn setting() -> RouteSetting {
        RouteSetting {
            places: places(),
            start_unix_s: NINE_UTC,
            ground_speed_kmh: 20.0,
            glide_ratio: 8.0,
            flying_height_m: None,
        }
    }

    #[test]
    fn the_path_names_the_places_and_the_day() {
        assert_eq!(
            route_weather_path(places(), "2026-11-22".to_owned()).unwrap(),
            "/v1/route/weather?date=2026-11-22&points=47.6642,11.7967;47.62,11.91;47.6603,11.5197"
        );
        assert!(route_weather_path(Vec::new(), "2026-11-22".to_owned()).is_err());
        assert!(route_weather_path(places(), "22.11.2026".to_owned()).is_err());
    }

    #[test]
    fn a_route_says_when_the_pilot_is_where_and_what_the_forecast_has_there() {
        assert_eq!(
            chrono::DateTime::from_timestamp(NINE_UTC, 0)
                .unwrap()
                .to_rfc3339(),
            "2026-11-22T09:00:00+00:00"
        );
        let forecast = RouteForecast::read(200, ANSWER.to_vec()).unwrap();
        let along = route_along(setting(), Some(forecast), None, 3600);

        assert_eq!(along.points.len(), 3);
        assert_eq!(along.legs.len(), 2);
        let first = &along.points[0];
        assert_eq!(first.clock, "10:00");
        assert_eq!(first.hour_unix_s, Some(NINE_UTC));
        // November: no thermals, and a wind read at a height.
        assert!(first.flyability.is_some());
        assert!(first.wind_height_m.is_some(), "{first:?}");
        assert!(first.wind_kmh.is_some(), "{first:?}");
        assert!(
            first.words.starts_with("Wallberg at 10:00: "),
            "{}",
            first.words
        );
        // The second place is some 10 km on: half an hour at 20 km/h.
        let second = &along.points[1];
        assert!(second.at_unix_s - NINE_UTC > 1500 && second.at_unix_s - NINE_UTC < 2100);
        // The Brauneck lies outside the map.
        let third = &along.points[2];
        assert_eq!(third.hour_unix_s, None);
        assert!(
            third.words.ends_with("outside the forecast's map."),
            "{}",
            third.words
        );
        // A leg's words name it, its length, its times and its wind.
        let leg = &along.legs[0];
        assert!(leg.head_wind_kmh.is_some() && leg.cross_wind_kmh.is_some());
        assert_eq!(
            along.words,
            "The route takes 1 h 58 at 20 km/h, from 10:00 to 11:58. Brauneck lies outside \
             the forecast's map. Nothing in the forecast rules out an hour on the way."
        );
        assert_eq!(
            first.words,
            "Wallberg at 10:00: no thermals. 9 km/h from the SSW at 1570 m. Overcast."
        );
        assert_eq!(
            leg.words,
            "The first leg, 9.8 km to Spitzing from 10:00 to 10:29: 8 km/h cross wind \
             from the right."
        );
        assert_eq!(along.model.as_deref(), Some("ICON-D2, run of 07:00"));
        assert_eq!(along.credits.len(), 1);
        // Without the ground no height to leave at is said, and why.
        assert_eq!(first.departure_m, None);
        assert!(
            first.departure_words.contains("is not loaded"),
            "{}",
            first.departure_words
        );
        assert!(along.points[2].departure_words.is_empty());
    }

    #[test]
    fn without_a_forecast_the_times_and_the_still_air_are_said() {
        let along = route_along(setting(), None, None, 0);
        assert_eq!(along.points[0].words, "Wallberg at 09:00.");
        assert_eq!(along.model, None);
        assert!(
            along.words.contains("No forecast along it"),
            "{}",
            along.words
        );
        assert!(along.legs[1].words.contains("no wind from the forecast"));
        // A place without a name is called by its number.
        let mut unnamed = setting();
        unnamed.places[1].name = None;
        let along = route_along(unnamed, None, None, 0);
        assert_eq!(along.points[1].name, "Point 2");
        assert!(
            along.legs[0].words.contains("to Point 2"),
            "{}",
            along.legs[0].words
        );
    }

    #[test]
    fn rain_a_closed_hour_and_a_head_wind_are_named_for_the_route() {
        let mut response: v1::RouteWeatherResponse = serde_json::from_slice(ANSWER).unwrap();
        for (index, point) in response.points.iter_mut().enumerate() {
            for hour in &mut point.hours {
                hour.wind_kmh = Some(20);
                hour.wind_from_deg = Some(120);
                for wind in &mut hour.aloft {
                    wind.kmh = 30;
                    wind.from_deg = Some(120);
                }
                if index == 1 {
                    hour.precipitation_mm = Some(1.2);
                    hour.verdict = Some(v1::ThermalVerdict::NotFlyable);
                    hour.why = Some(v1::ThermalReason::Rain);
                    hour.thermals = None;
                }
            }
        }
        let forecast = Arc::new(RouteForecast { response });
        let along = route_along(setting(), Some(forecast), None, 3600);

        let leg = &along.legs[0];
        assert_eq!(leg.rain_from_unix_s, Some(NINE_UTC));
        assert!(leg.head_wind_kmh.unwrap() > 25.0, "{leg:?}");
        assert!(
            leg.words
                .ends_with("km/h head wind. It runs into rain by 10:00."),
            "{}",
            leg.words
        );
        let spitzing = &along.points[1];
        assert_eq!(spitzing.flyability, Some(Flyability::NotFlyable));
        assert!(
            spitzing
                .words
                .starts_with("Spitzing at 10:29: not flyable by the forecast, rain."),
            "{}",
            spitzing.words
        );
        assert!(
            spitzing.words.ends_with("Rain, 1.2 mm in the hour."),
            "{}",
            spitzing.words
        );
        let words = &along.words;
        assert!(
            words.contains(" The first leg runs into rain by 10:00."),
            "{words}"
        );
        assert!(
            words.contains(" At Spitzing at 10:29 the forecast rules the air out."),
            "{words}"
        );
        assert!(
            words.contains(" The strongest head wind is on the leg to Spitzing, "),
            "{words}"
        );
        assert!(!words.contains("Nothing in the forecast"), "{words}");
    }

    /// The region that comes with the iPhone app: the ground around the
    /// Tegernsee, the Wallberg in it.
    fn tegernsee() -> Arc<Terrain> {
        let terrain = Terrain::new();
        let archive = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/ios/Venti/Resources/MapDemo/terrain.pmtiles"
        ))
        .unwrap();
        terrain.add_archive(archive).unwrap();
        terrain
    }

    #[test]
    fn a_point_says_how_high_to_leave_it_to_reach_the_next() {
        let mut short = setting();
        // From the Wallberg's top to Rottach-Egern on the lake, 4 km
        // north.
        short.places = vec![
            place(47.6642, 11.7967, "Wallberg"),
            place(47.6960, 11.7700, "Rottach"),
        ];
        let terrain = tegernsee();
        let still = route_along(short.clone(), None, Some(terrain.clone()), 3600);
        let height = still.points[0].departure_m.expect("a height");
        // The lake lies at 726 m: 200 m over it after 4 km at 8 is at
        // least 726 + 200 + 500 m; the mountain's own slope can ask for
        // more.
        assert!((1420.0..2200.0).contains(&height), "{height}");
        assert_eq!(height % 10.0, 0.0);
        assert_eq!(
            still.points[0].departure_words,
            format!(
                "Leave Wallberg at {height:.0} m to reach Rottach with 200 m to spare in still air."
            )
        );
        assert!(still.points[1].departure_words.is_empty());
    }

    #[test]
    fn a_leg_names_its_wind_by_its_larger_part() {
        let east = 90.0;
        let wind = |from_deg: f64, kmh: f64| Wind {
            from_deg,
            speed_ms: kmh / 3.6,
        };
        assert_eq!(wind_words(wind(90.0, 12.0), east), "12 km/h head wind");
        assert_eq!(wind_words(wind(270.0, 15.0), east), "15 km/h tail wind");
        assert_eq!(
            wind_words(wind(0.0, 20.0), east),
            "20 km/h cross wind from the left"
        );
        assert_eq!(
            wind_words(wind(200.0, 20.0), east),
            "19 km/h cross wind from the right"
        );
        assert_eq!(wind_words(wind(90.0, 3.0), east), "little wind, 3 km/h");
        assert_eq!(leg_name(1), "The second leg");
        assert_eq!(leg_name(11), "Leg 12");
    }

    #[test]
    fn the_wind_is_read_between_the_kept_heights_and_the_ground() {
        let hour = v1::RouteWeatherHour {
            wind_kmh: Some(10),
            wind_from_deg: Some(270),
            aloft: vec![
                v1::RouteWindAloft {
                    height_m: 1500,
                    kmh: 20,
                    from_deg: Some(270),
                },
                v1::RouteWindAloft {
                    height_m: 2000,
                    kmh: 30,
                    from_deg: Some(300),
                },
            ],
            ..v1::RouteWeatherHour::default()
        };
        let read =
            |height| wind_at(&hour, Some(990.0), height).map(|w| (w.speed_ms * 3.6, w.from_deg));
        let (kmh, from) = read(1750.0).unwrap();
        assert!((kmh - 25.0).abs() < 1e-9 && (from - 285.0).abs() < 1e-9);
        let (kmh, _) = read(1250.0).unwrap();
        assert!((kmh - 15.0).abs() < 1e-9, "{kmh}");
        assert_eq!(read(2100.0), None);
        assert_eq!(read(900.0), None);
    }
}
