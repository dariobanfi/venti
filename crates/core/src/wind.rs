//! Live wind from the Venti server.
//!
//! The core never talks to the network. For each thing a screen wants from
//! the server there is a pair here: a function that says what to ask for,
//! and one that reads the answer into a view. The shell does the request in
//! between, against whichever server the pilot has set.
//!
//! The wire format is `venti-api-types`, the same crate the server builds
//! its answers from.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use venti_api_types::v1::{
    Attribution, HEALTH_PATH, HealthResponse, ObservationsRequest, ObservationsResponse, Reading,
    RegistrationMode, StationWithLatest, StationsRequest, StationsResponse, format_day, parse_day,
};
use venti_domain::{BoundingBox, StationId};

use crate::server::answer;
use crate::{Coordinate, CoreError};

/// A reading older than this many minutes is shown as old. Ten-minute
/// stations whose provider publishes late are regularly 45 minutes behind;
/// beyond an hour a reading says little about the wind now.
const OLD_AFTER_MIN: i64 = 60;

/// Below this mean speed, in metres per second, the wind has no direction
/// worth an arrow: a station reporting it is drawn as calm.
const CALM_BELOW_MS: f64 = 0.5;

/// How many days before today a station's past days are offered. Ours: a
/// year, which a server that keeps observations for its default of
/// thirteen whole months always holds. A server set to keep less, and a
/// station the server has collected for less than a year, answer an older
/// day with no readings, which the day then says in words.
const STATION_DAYS_BACK: i64 = 365;

/// How strong the wind is, in four bands of its mean speed.
///
/// A magnitude, not a verdict: whether 25 km/h is too much depends on the
/// pilot, the wing and the place, and the app does not decide that from
/// one number. The shells give each band one step of a single hue, light
/// to dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum WindBand {
    /// Below 10 km/h.
    Light,
    /// From 10 to below 20 km/h.
    Moderate,
    /// From 20 to below 30 km/h.
    Fresh,
    /// 30 km/h and more.
    Strong,
}

/// The band a mean wind speed falls in.
#[uniffi::export]
pub fn wind_band(speed_ms: f64) -> WindBand {
    band_of_kmh(speed_ms * 3.6)
}

/// The band a speed in km/h falls in: for a speed that is held in km/h,
/// which a detour through m/s could push across a limit.
pub(crate) fn band_of_kmh(speed_kmh: f64) -> WindBand {
    match speed_kmh {
        speed if speed < 10.0 => WindBand::Light,
        speed if speed < 20.0 => WindBand::Moderate,
        speed if speed < 30.0 => WindBand::Fresh,
        _ => WindBand::Strong,
    }
}

/// What a station is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(tag = "kind", rename_all = "camelCase"))]
pub enum WindSymbol {
    /// An arrow in the band's colour, pointing where the wind blows.
    Arrow {
        /// How strong the wind is.
        band: WindBand,
    },
    /// Wind too weak to have a direction worth drawing.
    Calm,
    /// A reading too old to stand for the wind now, or none at all.
    Old,
}

/// How one station is drawn on a map and read in a list. The same on
/// every platform, so that a pilot who looks at the phone and then at the
/// browser sees the same wind.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindMark {
    /// What to draw.
    pub symbol: WindSymbol,
    /// Where an arrow points, in degrees clockwise from north: the
    /// direction the wind blows towards. Zero for the other symbols.
    pub heading_deg: f64,
    /// Mean wind and gusts in whole km/h, "12/25", or only "12" when the
    /// gusts are unknown or round to the same number. Empty for a station
    /// with nothing current to say.
    pub label: String,
}

/// Mean wind and gusts in whole km/h as a map and a list write them:
/// "12/25", or only "12" when the gusts are unknown or round to the same
/// number. Plain digits, without a region's separators.
#[uniffi::export]
pub fn wind_speeds_label(speed_ms: f64, gust_ms: Option<f64>) -> String {
    let whole = |speed: f64| (speed * 3.6).round() as i64;
    let mean = whole(speed_ms);
    match gust_ms.map(whole) {
        Some(gusts) if gusts != mean => format!("{mean}/{gusts}"),
        _ => mean.to_string(),
    }
}

fn wind_mark(latest: Option<&WindReading>, is_old: bool) -> WindMark {
    let current = latest
        .filter(|_| !is_old)
        .and_then(|reading| Some((reading, reading.speed_ms?)));
    let Some((reading, speed)) = current else {
        return WindMark {
            symbol: WindSymbol::Old,
            heading_deg: 0.0,
            label: String::new(),
        };
    };
    let label = wind_speeds_label(speed, reading.gust_ms);
    match reading.direction_deg {
        Some(from) if speed >= CALM_BELOW_MS => WindMark {
            symbol: WindSymbol::Arrow {
                band: wind_band(speed),
            },
            heading_deg: (from + 180.0).rem_euclid(360.0),
            label,
        },
        _ => WindMark {
            symbol: WindSymbol::Calm,
            heading_deg: 0.0,
            label,
        },
    }
}

/// One reading of a station, in the units the API uses.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindReading {
    /// End of the measuring interval, seconds since 1970-01-01 UTC.
    pub time_unix_s: i64,
    /// Mean wind speed, metres per second.
    pub speed_ms: Option<f64>,
    /// Strongest gust, metres per second.
    pub gust_ms: Option<f64>,
    /// The direction the wind comes from, degrees clockwise from north.
    pub direction_deg: Option<f64>,
    /// Air temperature, degrees Celsius.
    pub temperature_c: Option<f64>,
    /// Relative humidity, percent.
    pub humidity_pct: Option<f64>,
}

/// One station on the wind map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindStation {
    /// The station's identifier, to ask for its history.
    pub id: String,
    /// Its name. A name the provider publishes in capitals throughout is
    /// set in ordinary case; any other is as published.
    pub name: String,
    /// The data source it comes from; matches a [`Credit`]'s `source`.
    pub source: String,
    /// Where it stands.
    pub position: Coordinate,
    /// Its height above sea level, metres.
    pub elevation_m: Option<f64>,
    /// Its newest reading, if the server holds one.
    pub latest: Option<WindReading>,
    /// How old that reading is, in minutes.
    pub age_min: Option<i64>,
    /// Whether the reading is too old to stand for the wind now. The shell
    /// draws such a station greyed out.
    pub is_old: bool,
    /// How the station is drawn and labelled.
    pub mark: WindMark,
}

/// Whose data is shown. The licences require this credit next to the data.
/// A shell hands it back with a site the pilot keeps, so that the credit is
/// kept with it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct Credit {
    /// The data source it belongs to.
    pub source: String,
    /// Who publishes the data.
    pub provider: String,
    /// What the provider asks to be said wherever its data is shown, beside
    /// the credit: for example that the data is not certified.
    #[cfg_attr(feature = "wasm", serde(skip_serializing_if = "Option::is_none"))]
    pub notice: Option<String>,
    /// The credit line, worded as the provider asks.
    pub text: String,
    /// The licence's name.
    pub licence: String,
    /// Where the credit links to.
    pub url: String,
}

/// The wind map of one area.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindMap {
    /// The stations in the area.
    pub stations: Vec<WindStation>,
    /// Whose data they are.
    pub credits: Vec<Credit>,
    /// Whether the area holds more stations than one answer lists; the
    /// pilot should zoom in.
    pub truncated: bool,
}

/// What one station measured over time.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct StationHistory {
    /// The station.
    pub station: WindStation,
    /// Its readings, oldest first.
    pub readings: Vec<WindReading>,
    /// Whose data they are.
    pub credits: Vec<Credit>,
}

/// Who may create an account on a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum Registration {
    /// Anyone. No invite code is asked for.
    Open,
    /// Whoever has an invite code from whoever runs the server.
    Invite,
    /// Nobody. Existing accounts sign in as before.
    Closed,
}

/// How the server is doing.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ServerStatus {
    /// The server's version.
    pub version: String,
    /// The server's clock, seconds since 1970-01-01 UTC. The age of a
    /// reading is computed against this, not against the device's clock,
    /// which may be wrong.
    pub time_unix_s: i64,
    /// The providers whose data the server has not received lately. Their
    /// stations show old readings until the provider delivers again.
    pub late_providers: Vec<String>,
    /// Who may create an account here. `None` when the server does not
    /// say, as one older than the question, or names a rule this version
    /// does not know. Such a server may ask for an invite code, so a form
    /// for a new account then offers the field for one.
    pub registration: Option<Registration>,
}

/// The path to `GET` to learn how the server is doing.
#[uniffi::export]
pub fn server_status_path() -> String {
    HEALTH_PATH.to_owned()
}

/// Reads the server's answer to [`server_status_path`].
#[uniffi::export]
pub fn server_status(status: u16, body: Vec<u8>) -> Result<ServerStatus, CoreError> {
    let response: HealthResponse = answer(status, &body)?;
    Ok(ServerStatus {
        version: response.version,
        time_unix_s: response.time.timestamp(),
        late_providers: response
            .sources
            .into_iter()
            .filter(|source| source.stale)
            .map(|source| source.provider)
            .collect(),
        registration: match response.registration {
            Some(RegistrationMode::Open) => Some(Registration::Open),
            Some(RegistrationMode::Invite) => Some(Registration::Invite),
            Some(RegistrationMode::Closed) => Some(Registration::Closed),
            Some(RegistrationMode::Unknown) | None => None,
        },
    })
}

/// The path and query to `GET` from the server for the wind map of a box.
#[uniffi::export]
pub fn wind_map_path(south: f64, west: f64, north: f64, east: f64) -> Result<String, CoreError> {
    // A map can be dragged past the poles and the date line; ask for what
    // exists.
    let bbox = BoundingBox::new(
        south.clamp(-90.0, 90.0),
        west.clamp(-180.0, 180.0),
        north.clamp(-90.0, 90.0),
        east.clamp(-180.0, 180.0),
    )
    .map_err(|_| unreadable("this part of the map"))?;
    Ok(StationsRequest { bbox }.path_and_query())
}

/// Reads the server's answer to [`wind_map_path`]. `now_unix_s` is the
/// current time by the server's clock (see [`ServerStatus`]).
#[uniffi::export]
pub fn wind_map(status: u16, body: Vec<u8>, now_unix_s: i64) -> Result<WindMap, CoreError> {
    let response: StationsResponse = answer(status, &body)?;
    Ok(WindMap {
        stations: response
            .stations
            .iter()
            .map(|station| wind_station(station, now_unix_s))
            .collect(),
        credits: response.attributions.iter().map(credit).collect(),
        truncated: response.truncated,
    })
}

/// The path and query to `GET` for a station's readings of the last 24
/// hours.
#[uniffi::export]
pub fn station_history_path(station_id: String) -> Result<String, CoreError> {
    let station: StationId = station_id.parse().map_err(|_| unreadable("this station"))?;
    Ok(ObservationsRequest {
        station,
        from: None,
        to: None,
    }
    .path_and_query())
}

/// Reads the server's answer to [`station_history_path`].
#[uniffi::export]
pub fn station_history(
    status: u16,
    body: Vec<u8>,
    now_unix_s: i64,
) -> Result<StationHistory, CoreError> {
    let response: ObservationsResponse = answer(status, &body)?;
    let latest = response.observations.last().cloned();
    Ok(StationHistory {
        station: wind_station(
            &StationWithLatest {
                station: response.station,
                latest,
            },
            now_unix_s,
        ),
        readings: response.observations.iter().map(reading).collect(),
        credits: response.attributions.iter().map(credit).collect(),
    })
}

/// The days of a station's past that a day picker offers: from a year
/// back to today.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct StationDays {
    /// The earliest day offered, as [`station_day_path`] takes a day:
    /// "2025-11-21".
    pub first_date: String,
    /// The latest day offered, which is today: "2026-11-21".
    pub last_date: String,
    /// A moment inside the earliest day, its noon, seconds since
    /// 1970-01-01 UTC: for a picker that is bounded by moments.
    pub first_unix_s: i64,
    /// A moment inside the latest day, its noon.
    pub last_unix_s: i64,
    /// What to say beside the picker: how far back it reaches, and that a
    /// day may have no readings.
    pub note: String,
}

/// One day of a station's readings, chosen by the pilot.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct StationDay {
    /// The day, as it was asked for: "2026-11-18".
    pub date: String,
    /// When the day begins, seconds since 1970-01-01 UTC: where its chart
    /// starts.
    pub start_unix_s: i64,
    /// When it ends, 24 hours later: where its chart ends.
    pub end_unix_s: i64,
    /// The station's identifier.
    pub station_id: String,
    /// The station's name, as a [`WindStation`] has it.
    pub station_name: String,
    /// Its readings of the day, oldest first.
    pub readings: Vec<WindReading>,
    /// Whose data they are.
    pub credits: Vec<Credit>,
    /// What to say in place of a chart when the day has no readings.
    /// `None` when it has some.
    pub empty_words: Option<String>,
}

/// The day a moment falls on, as [`station_day_path`] takes a day:
/// "2026-11-18". `utc_offset_s` is how far the pilot's clock is ahead of
/// UTC at that moment, in seconds. For a day picker that hands out a
/// moment and not a date.
#[uniffi::export]
pub fn station_date(at_unix_s: i64, utc_offset_s: i32) -> String {
    format_day(local_day(at_unix_s, utc_offset_s))
}

/// The days of a station's past to offer in a day picker. `now_unix_s` is
/// the current time by the server's clock (see [`ServerStatus`]) and
/// `utc_offset_s` how far the pilot's clock is ahead of UTC, in seconds:
/// today is that clock's day.
#[uniffi::export]
pub fn station_days(now_unix_s: i64, utc_offset_s: i32) -> StationDays {
    let today = local_day(now_unix_s, utc_offset_s);
    let first = today - Duration::days(STATION_DAYS_BACK);
    let noon = |day: NaiveDate| (day_start(day, utc_offset_s) + Duration::hours(12)).timestamp();
    StationDays {
        first_date: format_day(first),
        last_date: format_day(today),
        first_unix_s: noon(first),
        last_unix_s: noon(today),
        note: "A station's readings are shown for the last year. A day on which the station \
               was silent, or which the server did not collect, has none."
            .to_owned(),
    }
}

/// The path and query to `GET` for one day of a station's readings.
///
/// `date` names the day as a calendar date, "2026-11-18": the text a web
/// page's date field holds and [`station_date`] makes from a moment.
/// `utc_offset_s` is how far the pilot's clock was ahead of UTC on that
/// day, in seconds: the day runs from that clock's midnight for 24 hours.
/// On the two days a year the clocks change that is an hour off at one
/// end.
///
/// Which days to offer is [`station_days`]'s to say. This asks for any
/// day; the server answers one it does not hold with no readings.
#[uniffi::export]
pub fn station_day_path(
    station_id: String,
    date: String,
    utc_offset_s: i32,
) -> Result<String, CoreError> {
    let station: StationId = station_id.parse().map_err(|_| unreadable("this station"))?;
    let day = parse_day(&date).map_err(|_| unreadable("this day"))?;
    let start = day_start(day, utc_offset_s);
    Ok(ObservationsRequest {
        station,
        from: Some(start),
        to: Some(start + Duration::days(1)),
    }
    .path_and_query())
}

/// Reads the server's answer to [`station_day_path`], which was asked
/// with the same `date` and `utc_offset_s`.
#[uniffi::export]
pub fn station_day(
    status: u16,
    body: Vec<u8>,
    date: String,
    utc_offset_s: i32,
) -> Result<StationDay, CoreError> {
    let response: ObservationsResponse = answer(status, &body)?;
    let day = parse_day(&date).map_err(|_| unreadable("this day"))?;
    let start = day_start(day, utc_offset_s);
    let end = start + Duration::days(1);
    let readings: Vec<WindReading> = response
        .observations
        .iter()
        // An answer to another question than this day's is not drawn as
        // this day.
        .filter(|observation| (start..=end).contains(&observation.time))
        .map(reading)
        .collect();
    Ok(StationDay {
        date: format_day(day),
        start_unix_s: start.timestamp(),
        end_unix_s: end.timestamp(),
        station_id: response.station.id.to_string(),
        station_name: readable_name(&response.station.name),
        empty_words: readings
            .is_empty()
            .then(|| "The server has no readings of this station for this day.".to_owned()),
        readings,
        credits: response.attributions.iter().map(credit).collect(),
    })
}

/// The calendar day a moment falls on by a clock that is `utc_offset_s`
/// ahead of UTC.
fn local_day(at_unix_s: i64, utc_offset_s: i32) -> NaiveDate {
    let at: DateTime<Utc> = DateTime::from_timestamp(at_unix_s, 0).unwrap_or_default();
    (at + Duration::seconds(i64::from(utc_offset_s))).date_naive()
}

/// The moment a calendar day begins by a clock that is `utc_offset_s`
/// ahead of UTC.
fn day_start(day: NaiveDate, utc_offset_s: i32) -> DateTime<Utc> {
    day.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc()
        - Duration::seconds(i64::from(utc_offset_s))
}

fn unreadable(what: &str) -> CoreError {
    CoreError::Server {
        message: format!("The app could not ask the server about {what}."),
    }
}

fn reading(reading: &Reading) -> WindReading {
    WindReading {
        time_unix_s: reading.time.timestamp(),
        speed_ms: reading.wind_speed_ms,
        gust_ms: reading.wind_gust_ms,
        direction_deg: reading.wind_direction_deg,
        temperature_c: reading.temperature_c,
        humidity_pct: reading.humidity_pct,
    }
}

fn wind_station(station: &StationWithLatest, now_unix_s: i64) -> WindStation {
    let now: DateTime<Utc> = DateTime::from_timestamp(now_unix_s, 0).unwrap_or_default();
    let age_min = station
        .latest
        .as_ref()
        .map(|latest| (now - latest.time).num_minutes().max(0));
    let latest = station.latest.as_ref().map(reading);
    let is_old = age_min.is_none_or(|age| age > OLD_AFTER_MIN);
    WindStation {
        id: station.station.id.to_string(),
        name: readable_name(&station.station.name),
        source: station.station.source.to_string(),
        position: Coordinate {
            lat: station.station.position.lat(),
            lon: station.station.position.lon(),
        },
        elevation_m: station.station.elevation_m,
        mark: wind_mark(latest.as_ref(), is_old),
        latest,
        age_min,
        is_old,
    }
}

/// Some providers publish station names in capitals throughout
/// ("PATSCHERKOFEL"), which shouts in a list. Such a name is set with a
/// capital only at the start of each word; a name that already has small
/// letters is left exactly as published.
fn readable_name(name: &str) -> String {
    if name.chars().any(char::is_lowercase) {
        return name.to_owned();
    }
    let mut readable = String::with_capacity(name.len());
    let mut in_word = false;
    for letter in name.chars() {
        if in_word {
            readable.extend(letter.to_lowercase());
        } else {
            readable.push(letter);
        }
        in_word = letter.is_alphabetic();
    }
    readable
}

pub(crate) fn credit(attribution: &Attribution) -> Credit {
    Credit {
        source: attribution.source.to_string(),
        provider: attribution.provider.clone(),
        notice: attribution.notice.clone(),
        text: attribution.text.clone(),
        licence: attribution.licence.clone(),
        url: attribution.url.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 18 November 2026, 19:05 UTC: the moment the server's development
    /// data was recorded.
    const NOW: i64 = 1_795_028_700;

    const STATIONS: &str = r#"{
        "stations": [
            {
                "id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
                "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.0,
                "latest": {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9, "wind_gust_ms": 25.7,
                           "wind_direction_deg": 260.0, "temperature_c": -7.1, "a_field_from_the_future": 1}
            },
            {
                "id": "geosphere-tawes:11121", "source": "geosphere-tawes", "name": "Innsbruck-Flughafen",
                "position": {"lat": 47.26, "lon": 11.357},
                "latest": {"time": "2026-11-18T16:00:00Z", "wind_speed_ms": 2.1}
            },
            {
                "id": "geosphere-tawes:11999", "source": "geosphere-tawes", "name": "Silent",
                "position": {"lat": 47.3, "lon": 11.4}
            }
        ],
        "truncated": false,
        "attributions": [
            {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "licence": "CC BY 4.0",
             "text": "Quelle: Deutscher Wetterdienst", "url": "https://www.dwd.de/"}
        ]
    }"#;

    #[test]
    fn the_wind_map_is_asked_for_by_box() {
        assert_eq!(
            wind_map_path(47.0, 10.9, 47.5, 11.5).unwrap(),
            "/v1/stations?bbox=10.9,47,11.5,47.5"
        );
        // A map dragged past the edge of the world still asks a valid question.
        assert_eq!(
            wind_map_path(80.0, 170.0, 95.0, 190.0).unwrap(),
            "/v1/stations?bbox=170,80,180,90"
        );
    }

    #[test]
    fn stations_are_read_with_their_age_and_fields_from_newer_servers_are_ignored() {
        let map = wind_map(200, STATIONS.as_bytes().to_vec(), NOW).unwrap();

        assert_eq!(map.stations.len(), 3);
        let zugspitze = &map.stations[0];
        assert_eq!(zugspitze.id, "dwd-10min:05792");
        assert_eq!(zugspitze.name, "Zugspitze");
        assert_eq!(zugspitze.elevation_m, Some(2956.0));
        let latest = zugspitze.latest.unwrap();
        assert_eq!(latest.speed_ms, Some(14.9));
        assert_eq!(latest.gust_ms, Some(25.7));
        assert_eq!(latest.direction_deg, Some(260.0));
        // Measured at 18:20, read at 19:05.
        assert_eq!(zugspitze.age_min, Some(45));
        assert!(!zugspitze.is_old);

        // Three hours old, and one that never reported: both shown as old.
        assert_eq!(map.stations[1].age_min, Some(185));
        assert!(map.stations[1].is_old);
        assert_eq!(map.stations[1].latest.unwrap().gust_ms, None);
        assert_eq!(map.stations[2].latest, None);
        assert!(map.stations[2].is_old);

        assert_eq!(map.credits.len(), 1);
        assert_eq!(map.credits[0].text, "Quelle: Deutscher Wetterdienst");
        assert!(!map.truncated);
    }

    #[test]
    fn a_station_is_marked_by_its_wind_the_same_way_everywhere() {
        let map = wind_map(200, STATIONS.as_bytes().to_vec(), NOW).unwrap();

        // 54 km/h from the west with gusts of 93: a dark arrow pointing east.
        let summit = &map.stations[0].mark;
        assert_eq!(
            summit.symbol,
            WindSymbol::Arrow {
                band: WindBand::Strong
            }
        );
        assert_eq!(summit.heading_deg, 80.0);
        assert_eq!(summit.label, "54/93");

        // An old reading and no reading say nothing about the wind now.
        for silent in &map.stations[1..] {
            assert_eq!(silent.mark.symbol, WindSymbol::Old);
            assert_eq!(silent.mark.label, "");
        }

        let reading = |speed_ms, direction_deg| WindReading {
            time_unix_s: NOW,
            speed_ms,
            gust_ms: None,
            direction_deg,
            temperature_c: None,
            humidity_pct: None,
        };
        // Too weak for a direction, or without one: calm, with its speed.
        let still = wind_mark(Some(&reading(Some(0.4), Some(90.0))), false);
        assert_eq!(
            (still.symbol, still.label.as_str()),
            (WindSymbol::Calm, "1")
        );
        let turning = wind_mark(Some(&reading(Some(3.0), None)), false);
        assert_eq!(
            (turning.symbol, turning.label.as_str()),
            (WindSymbol::Calm, "11")
        );
        // A station that reports no speed has nothing to draw.
        assert_eq!(
            wind_mark(Some(&reading(None, Some(90.0))), false).symbol,
            WindSymbol::Old
        );
        // A north wind blows south; a direction of 360 is north too.
        assert_eq!(
            wind_mark(Some(&reading(Some(5.0), Some(0.0))), false).heading_deg,
            180.0
        );
        assert_eq!(
            wind_mark(Some(&reading(Some(5.0), Some(360.0))), false).heading_deg,
            180.0
        );
    }

    #[test]
    fn wind_falls_into_four_bands_and_is_written_in_whole_kilometres_per_hour() {
        assert_eq!(wind_band(0.0), WindBand::Light);
        assert_eq!(wind_band(2.7), WindBand::Light);
        assert_eq!(wind_band(2.8), WindBand::Moderate);
        assert_eq!(wind_band(5.5), WindBand::Moderate);
        assert_eq!(wind_band(5.6), WindBand::Fresh);
        assert_eq!(wind_band(8.3), WindBand::Fresh);
        assert_eq!(wind_band(8.4), WindBand::Strong);

        assert_eq!(wind_speeds_label(3.0, None), "11");
        assert_eq!(wind_speeds_label(3.0, Some(7.0)), "11/25");
        // Gusts that round to the mean add nothing.
        assert_eq!(wind_speeds_label(3.0, Some(3.1)), "11");
    }

    #[test]
    fn names_in_capitals_are_set_in_ordinary_case() {
        assert_eq!(readable_name("PATSCHERKOFEL"), "Patscherkofel");
        assert_eq!(
            readable_name("INNSBRUCK/UNIVERSITAET"),
            "Innsbruck/Universitaet"
        );
        assert_eq!(readable_name("SONNBLICK - AUTOM."), "Sonnblick - Autom.");
        assert_eq!(readable_name("ST.PÖLTEN LANDHAUS"), "St.Pölten Landhaus");
        // Names that already have small letters are the provider's business.
        assert_eq!(
            readable_name("Garmisch-Partenkirchen"),
            "Garmisch-Partenkirchen"
        );
        assert_eq!(readable_name("Zugspitze (DWD)"), "Zugspitze (DWD)");
        assert_eq!(readable_name(""), "");
    }

    #[test]
    fn a_station_history_is_asked_for_and_read() {
        assert_eq!(
            station_history_path("dwd-10min:05792".to_owned()).unwrap(),
            "/v1/stations/dwd-10min:05792/observations"
        );
        assert!(station_history_path("not an id".to_owned()).is_err());

        let body = r#"{
            "station": {"id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
                        "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.0},
            "observations": [
                {"time": "2026-11-18T18:10:00Z", "wind_speed_ms": 13.0, "wind_gust_ms": 22.0},
                {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9, "wind_gust_ms": 25.7}
            ],
            "attributions": []
        }"#;
        let history = station_history(200, body.as_bytes().to_vec(), NOW).unwrap();

        assert_eq!(history.readings.len(), 2);
        assert!(history.readings[0].time_unix_s < history.readings[1].time_unix_s);
        assert_eq!(history.station.latest, Some(history.readings[1]));
        assert_eq!(history.station.age_min, Some(45));
    }

    #[test]
    fn a_reading_carries_the_temperature_and_the_humidity_the_station_sent() {
        let body = r#"{
            "stations": [{
                "id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
                "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.0,
                "latest": {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9,
                           "temperature_c": -7.1, "humidity_pct": 91.8, "pressure_hpa": 693.4}
            }],
            "truncated": false, "attributions": []
        }"#;
        let map = wind_map(200, body.as_bytes().to_vec(), NOW).unwrap();
        let latest = map.stations[0].latest.unwrap();
        assert_eq!(latest.temperature_c, Some(-7.1));
        assert_eq!(latest.humidity_pct, Some(91.8));
        // A station that sends none has none.
        let map = wind_map(200, STATIONS.as_bytes().to_vec(), NOW).unwrap();
        assert_eq!(map.stations[0].latest.unwrap().humidity_pct, None);
    }

    #[test]
    fn a_day_picker_is_offered_the_last_year_by_the_pilots_clock() {
        // 19:05 UTC on 18 November is 20:05 on a clock an hour ahead.
        let days = station_days(NOW, 3600);
        assert_eq!(days.last_date, "2026-11-18");
        assert_eq!(days.first_date, "2025-11-18");
        // Noon of each day by that clock, which is 11:00 UTC: a moment
        // inside the day for a picker that is bounded by moments.
        assert_eq!(days.last_unix_s, NOW - (8 * 3600 + 5 * 60));
        assert_eq!(days.last_unix_s - days.first_unix_s, 365 * 86_400);
        assert_eq!(station_date(days.first_unix_s, 3600), days.first_date);
        assert_eq!(station_date(days.last_unix_s, 3600), days.last_date);
        assert!(days.note.contains("last year"), "{}", days.note);

        // Five hours ahead it is already the 19th; ten hours behind, and at
        // Greenwich, still the 18th.
        assert_eq!(station_days(NOW, 5 * 3600).last_date, "2026-11-19");
        assert_eq!(station_days(NOW, -10 * 3600).last_date, "2026-11-18");
        assert_eq!(station_date(NOW, 5 * 3600), "2026-11-19");
        assert_eq!(station_date(NOW, 0), "2026-11-18");
        // Half an hour after midnight UTC, clocks behind UTC have the day
        // before.
        assert_eq!(station_date(NOW + 5 * 3600 + 25 * 60, -3600), "2026-11-18");
    }

    #[test]
    fn a_day_of_a_station_is_asked_for_from_midnight_to_midnight_by_the_pilots_clock() {
        let path = |date: &str, utc_offset_s| {
            station_day_path("dwd-10min:05792".to_owned(), date.to_owned(), utc_offset_s)
        };
        assert_eq!(
            path("2026-11-17", 3600).unwrap(),
            "/v1/stations/dwd-10min:05792/observations\
             ?from=2026-11-16T23:00:00Z&to=2026-11-17T23:00:00Z"
        );
        // In summer the same clock is two hours ahead.
        assert_eq!(
            path("2026-07-04", 7200).unwrap(),
            "/v1/stations/dwd-10min:05792/observations\
             ?from=2026-07-03T22:00:00Z&to=2026-07-04T22:00:00Z"
        );
        assert_eq!(
            path("2026-11-17", -5 * 3600).unwrap(),
            "/v1/stations/dwd-10min:05792/observations\
             ?from=2026-11-17T05:00:00Z&to=2026-11-18T05:00:00Z"
        );

        // Only a date as a date field writes it, and only a station.
        for not_a_day in ["17.11.2026", "2026-11-31", "2026-1-5", "yesterday", ""] {
            let error = path(not_a_day, 3600).unwrap_err();
            assert!(error.to_string().contains("this day"), "{error}");
        }
        assert!(
            station_day_path("not an id".to_owned(), "2026-11-17".to_owned(), 0)
                .unwrap_err()
                .to_string()
                .contains("this station")
        );
    }

    #[test]
    fn a_day_of_a_station_is_read_with_where_its_chart_begins_and_ends() {
        let body = r#"{
            "station": {"id": "geosphere-tawes:11121", "source": "geosphere-tawes",
                        "name": "INNSBRUCK-FLUGHAFEN",
                        "position": {"lat": 47.26, "lon": 11.357}, "elevation_m": 578.0},
            "observations": [
                {"time": "2026-11-16T22:50:00Z", "wind_speed_ms": 1.0},
                {"time": "2026-11-16T23:00:00Z", "wind_speed_ms": 1.5, "temperature_c": 2.1,
                 "humidity_pct": 88.0},
                {"time": "2026-11-17T12:00:00Z", "wind_speed_ms": 4.0, "wind_gust_ms": 7.5,
                 "wind_direction_deg": 250.0, "temperature_c": 9.4, "humidity_pct": 52.0},
                {"time": "2026-11-17T23:00:00Z", "wind_speed_ms": 2.0}
            ],
            "attributions": [
                {"source": "geosphere-tawes", "provider": "GeoSphere Austria",
                 "licence": "CC BY 4.0", "text": "Quelle: GeoSphere Austria",
                 "url": "https://www.geosphere.at/"}
            ]
        }"#;
        let day =
            station_day(200, body.as_bytes().to_vec(), "2026-11-17".to_owned(), 3600).unwrap();

        assert_eq!(day.date, "2026-11-17");
        // Midnight to midnight on a clock an hour ahead of UTC.
        assert_eq!(day.end_unix_s - day.start_unix_s, 86_400);
        assert_eq!(station_date(day.start_unix_s, 3600), "2026-11-17");
        assert_eq!(station_date(day.start_unix_s - 1, 3600), "2026-11-16");
        assert_eq!(
            (day.station_id.as_str(), day.station_name.as_str()),
            ("geosphere-tawes:11121", "Innsbruck-Flughafen")
        );
        // Both midnights belong to the day's chart; a reading from before
        // it, which this day was not asked for, does not.
        assert_eq!(day.readings.len(), 3);
        assert_eq!(day.readings[0].time_unix_s, day.start_unix_s);
        assert_eq!(day.readings[2].time_unix_s, day.end_unix_s);
        assert_eq!(day.readings[1].gust_ms, Some(7.5));
        assert_eq!(day.readings[1].humidity_pct, Some(52.0));
        assert_eq!(day.empty_words, None);
        assert_eq!(day.credits[0].text, "Quelle: GeoSphere Austria");

        // A day the server holds nothing of is said in words, not drawn as
        // an empty chart.
        let silent = r#"{
            "station": {"id": "geosphere-tawes:11121", "source": "geosphere-tawes",
                        "name": "INNSBRUCK-FLUGHAFEN",
                        "position": {"lat": 47.26, "lon": 11.357}},
            "observations": [],
            "attributions": []
        }"#;
        let day = station_day(
            200,
            silent.as_bytes().to_vec(),
            "2025-11-18".to_owned(),
            3600,
        )
        .unwrap();
        assert_eq!(day.date, "2025-11-18");
        assert!(day.readings.is_empty());
        assert_eq!(
            day.empty_words.as_deref(),
            Some("The server has no readings of this station for this day.")
        );

        // The server's refusal and a date that is none are errors in words.
        let refused = r#"{"error": {"code": "invalid_range", "message": "x"}}"#;
        let error =
            station_day(400, refused.as_bytes().to_vec(), "2026-11-17".to_owned(), 0).unwrap_err();
        assert!(error.to_string().contains("invalid_range"), "{error}");
        assert!(station_day(200, body.as_bytes().to_vec(), "17.11.".to_owned(), 0).is_err());
    }

    #[test]
    fn the_servers_clock_and_late_providers_are_read() {
        assert_eq!(server_status_path(), "/v1/health");
        let body = r#"{
            "status": "degraded", "version": "0.1.0", "time": "2026-11-18T19:05:00Z",
            "sources": [
                {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "cadence_seconds": 600,
                 "stale": true, "stations": 3},
                {"source": "geosphere-tawes", "provider": "GeoSphere Austria", "cadence_seconds": 600,
                 "last_success": "2026-11-18T19:05:00Z", "stale": false, "stations": 3}
            ]
        }"#;
        let status = server_status(200, body.as_bytes().to_vec()).unwrap();

        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.time_unix_s, NOW);
        assert_eq!(status.late_providers, ["Deutscher Wetterdienst"]);
        // A server older than the question says nothing about who may
        // register.
        assert_eq!(status.registration, None);
    }

    #[test]
    fn who_may_register_is_read_and_a_rule_not_known_is_no_answer() {
        let says = |registration: &str| {
            let body = format!(
                r#"{{"status": "ok", "version": "0.1.0", "time": "2026-11-18T19:05:00Z",
                    "registration": "{registration}", "sources": []}}"#
            );
            server_status(200, body.into_bytes()).unwrap().registration
        };

        assert_eq!(says("open"), Some(Registration::Open));
        assert_eq!(says("invite"), Some(Registration::Invite));
        assert_eq!(says("closed"), Some(Registration::Closed));
        // A rule a later server may have: taken as "does not say", which
        // leaves the field for an invite code on the form.
        assert_eq!(says("by_email"), None);
    }

    /// The server's own recorded answers, which its tests keep current.
    /// Reading them here catches a wire format that drifts away from what
    /// the apps understand.
    fn golden(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    #[test]
    fn the_servers_recorded_answers_are_understood() {
        let status = server_status(200, golden("health.json")).unwrap();
        assert_eq!(status.time_unix_s, NOW);
        assert!(status.late_providers.is_empty());
        // The development mode takes anyone.
        assert_eq!(status.registration, Some(Registration::Open));

        let map = wind_map(200, golden("stations.json"), status.time_unix_s).unwrap();
        assert!(!map.stations.is_empty());
        assert!(map.stations.iter().all(|station| station.latest.is_some()));
        // Recorded with every provider on time, so nothing is shown as old.
        assert!(map.stations.iter().all(|station| !station.is_old));
        // Every station's source has its credit in the same answer.
        for station in &map.stations {
            assert!(
                map.credits
                    .iter()
                    .any(|credit| credit.source == station.source),
                "{} has no credit",
                station.name
            );
        }

        let history =
            station_history(200, golden("observations.json"), status.time_unix_s).unwrap();
        assert!(history.readings.len() > 1);
        assert_eq!(history.station.latest, history.readings.last().copied());
        assert_eq!(history.credits.len(), 1);

        // The same answer read as the day it was recorded on, by UTC.
        let day = station_day(
            200,
            golden("observations.json"),
            station_date(status.time_unix_s, 0),
            0,
        )
        .unwrap();
        assert_eq!(day.readings, history.readings);
        assert_eq!(day.station_id, history.station.id);
        assert_eq!(day.empty_words, None);
        // The recorded stations send what a cloud base is worked out from.
        assert!(
            day.readings.iter().all(|reading| {
                reading.temperature_c.is_some() && reading.humidity_pct.is_some()
            })
        );

        let refused = wind_map(400, golden("error_missing_bbox.json"), NOW).unwrap_err();
        assert!(refused.to_string().contains("invalid_bbox"));
    }

    #[test]
    fn failures_are_put_in_words_for_the_pilot() {
        let error = |status: u16, body: &str| {
            wind_map(status, body.as_bytes().to_vec(), NOW)
                .unwrap_err()
                .to_string()
        };

        let said = error(
            400,
            r#"{"error": {"code": "invalid_bbox", "message": "the bbox parameter is missing"}}"#,
        );
        assert!(said.contains("invalid_bbox"), "{said}");
        assert!(!said.contains("parameter"), "{said}");
        // A code from a newer server is passed on as it is.
        assert!(
            error(429, r#"{"error": {"code": "slow_down", "message": "x"}}"#).contains("slow_down")
        );
        assert!(error(503, "").contains("Try again"));
        assert!(error(502, "<html>Bad Gateway</html>").contains("502"));
        assert!(error(200, "not json").contains("could not be read"));
    }
}
