//! Adapter for the weather stations of ARPA Lombardia, the environmental
//! agency of the Region of Lombardy: from Lake Maggiore to Lake Garda and
//! from the Po up to Livigno and the Stelvio.
//!
//! The agency's data is on the region's open-data portal,
//! `www.dati.lombardia.it`, as two datasets of a Socrata portal: the sensors
//! (`nf78-nj6b`: a row a sensor, with its kind, unit, station, name, height
//! and position) and the values of the month (`647i-nhxk`: a row a sensor,
//! time and operator, with the value and its state). A station is several
//! sensors, each with an id of its own: wind speed, wind direction,
//! temperature and humidity. The values name the sensor only, and the
//! portal joins no datasets ("joins are not supported", it answered on
//! 2026-11-22).
//!
//! So a run is two steps. It asks for the sensors of the four kinds in the
//! run's area, and of them keeps the stations that have an anemometer. From
//! that list it writes the values' query, in which the portal itself puts
//! each value with its station and kind: a `case` that names, for every
//! sensor asked, `<station>/<kind>`. The adapter then needs nothing but the
//! answer to read a value, and stays pure. A query that names some 500
//! sensors is longer than the portal takes in one address (it answered
//! `414` to 16,899 bytes and took 13,328), so the sensors are asked for in
//! groups of whole stations, each address at most [`MAX_URL_BYTES`] long:
//! three requests for all of Lombardy, two for the southern pack's box.
//!
//! What a value is, by the datasets' own records on the portal (read
//! 2026-11-22):
//!
//! - `data`: the time, "ora solare", the value referring to the observations
//!   "ottenute fino all'orario indicato": the end of its ten minutes, in
//!   UTC+1 the whole year. That the clock is UTC+1 and not UTC was checked
//!   against the sun: the global radiation of 30 of the network's sensors on
//!   2026-11-21 rose at 07:30 and peaked at 12:10 to 12:20 by that clock,
//!   when the sun stood highest over Lombardy at about 11:10 UTC.
//! - `idoperatore`: "1: Valore medio", "3: Valore massimo". A wind speed
//!   sensor has both each ten minutes: the mean, and the maximum, which is
//!   taken as the gust; a direction sensor has both too, the second taken as
//!   the gust's direction. Over how long the maximum is taken is not written
//!   anywhere that was found: *to verify*.
//! - `valore`: "-9999 = dato mancante", and for the direction "888, 8888 =
//!   direzione vento variabile", "777, 7777 = calma". None of the three is a
//!   direction, and the adapter takes them as no value.
//! - `stato`: "VA, VV = dato valido", "NA, NV, NC = dato invalido", "NI =
//!   dato incerto", "ND = dato non disponibile". Only a valid value is taken.
//!
//! The units are the sensors' (`unit_dimisura`): m/s, degrees, °C and
//! percent, each the canonical one, so no value is converted but the time;
//! a sensor that names another unit is not taken. The network has no
//! pressure.

pub mod fixtures;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use chrono::{Duration, NaiveDateTime, Timelike};
use venti_domain::{BoundingBox, GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

const PORTAL: &str = "https://www.dati.lombardia.it/resource";

/// The longest address of a request for values, in bytes: under two thirds
/// of the longest the portal was seen to take.
pub const MAX_URL_BYTES: usize = 8000;

/// How far back a run asks for values, in hours, by the values' time. The
/// portal takes the values in four loads a day, and the first values of a
/// ten minutes arrived up to 385 minutes after it in November 2026 (see the
/// manifest): seven hours take in the latest of them.
pub const WINDOW_HOURS: i64 = 7;

/// The most rows one answer is asked to hold. The portal answers with 1000
/// when it is not told; seven hours of all of Lombardy's sensors are at most some
/// 32,000.
pub const ROW_LIMIT: usize = 50_000;

/// The most sensors the list is asked for. Lombardy has 506 of the four
/// kinds at stations with an anemometer.
const SENSOR_LIMIT: usize = 5000;

/// The kinds of sensor taken, by the portal's name, with their units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Speed,
    Direction,
    Temperature,
    Humidity,
}

impl Kind {
    const ALL: [Self; 4] = [
        Self::Speed,
        Self::Direction,
        Self::Temperature,
        Self::Humidity,
    ];

    /// The portal's `tipologia`.
    fn portal_name(self) -> &'static str {
        match self {
            Self::Speed => "Velocità Vento",
            Self::Direction => "Direzione Vento",
            Self::Temperature => "Temperatura",
            Self::Humidity => "Umidità Relativa",
        }
    }

    /// The portal's `unit_dimisura`, which is the canonical unit.
    fn unit(self) -> &'static str {
        match self {
            Self::Speed => "m/s",
            Self::Direction => "°",
            Self::Temperature => "°C",
            Self::Humidity => "%",
        }
    }

    /// The letter the values' query names the kind by.
    fn letter(self) -> char {
        match self {
            Self::Speed => 'v',
            Self::Direction => 'd',
            Self::Temperature => 't',
            Self::Humidity => 'u',
        }
    }

    fn from_portal_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.portal_name() == name)
    }

    fn from_letter(letter: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| letter.len() == 1 && letter.starts_with(kind.letter()))
    }
}

/// Writes a part of a query for an address: letters, digits and the few
/// signs the portal reads as they are stay, everything else is
/// percent-encoded, byte by byte of its UTF-8.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~,()*/:=".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The address of the sensors of the four kinds that are in service and
/// stand in `area`.
pub fn sensors_url(area: &BoundingBox) -> String {
    let kinds: Vec<String> = Kind::ALL
        .iter()
        .map(|kind| format!("'{}'", kind.portal_name()))
        .collect();
    let filter = format!(
        "storico='N' AND tipologia in({}) AND lat between {} and {} AND lng between {} and {}",
        kinds.join(","),
        area.south(),
        area.north(),
        area.west(),
        area.east(),
    );
    format!(
        "{PORTAL}/nf78-nj6b.json?$select=idsensore,tipologia,unit_dimisura,idstazione,nomestazione,quota,lat,lng&$where={}&$order=idsensore&$limit={SENSOR_LIMIT}",
        encode(&filter)
    )
}

/// The moment from which a run at `now` asks for values, in the values'
/// own clock (UTC+1), on the ten minutes.
fn since(now: Timestamp) -> String {
    let local = (now + Duration::hours(1) - Duration::hours(WINDOW_HOURS)).naive_utc();
    let local = local
        .with_minute(local.minute() / 10 * 10)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(local);
    local.format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// One sensor as the values' query names it.
#[derive(Debug, Clone)]
struct Sensor {
    id: String,
    station: String,
    kind: Kind,
}

impl Sensor {
    fn arm(&self) -> String {
        format!(
            "idsensore='{}','{}/{}'",
            self.id,
            self.station,
            self.kind.letter()
        )
    }
}

/// The address of the values since [`WINDOW_HOURS`] before `now` of the
/// sensors named, each row with its station and kind.
fn values_url(sensors: &[Sensor], now: Timestamp) -> String {
    let arms: Vec<String> = sensors.iter().map(Sensor::arm).collect();
    let query = format!(
        "SELECT data,valore,idoperatore,stato,case({}) AS s WHERE data >= '{}' |> SELECT * WHERE s IS NOT NULL ORDER BY data,s,idoperatore LIMIT {ROW_LIMIT}",
        arms.join(","),
        since(now),
    );
    format!("{PORTAL}/647i-nhxk.json?$query={}", encode(&query))
}

/// The requests for the values of `stations`' sensors: whole stations in
/// each, as many as fit in [`MAX_URL_BYTES`].
fn values_requests(stations: &[Vec<Sensor>], now: Timestamp) -> Vec<Request> {
    let mut groups: Vec<Vec<Sensor>> = Vec::new();
    let mut current: Vec<Sensor> = Vec::new();
    for sensors in stations {
        let mut tried = current.clone();
        tried.extend(sensors.iter().cloned());
        if !current.is_empty() && values_url(&tried, now).len() > MAX_URL_BYTES {
            groups.push(std::mem::take(&mut current));
            current.extend(sensors.iter().cloned());
        } else {
            current = tried;
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
        .iter()
        .enumerate()
        .map(|(index, group)| Request::new(format!("values/{}", index + 1), values_url(group, now)))
        .collect()
}

/// ARPA Lombardia's stations as an observation source.
#[derive(Debug, Clone)]
pub struct ArpaLombardia {
    manifest: Manifest,
}

impl ArpaLombardia {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-arpalombardia is valid");
        Self { manifest }
    }
}

/// The network as a build takes it in: the adapter and a sample of the
/// evening the server's development mode stands at. The server's list of
/// adapters has one line for this function and names nothing else of the
/// crate.
///
/// No region pack names this network: its values reach the portal two to
/// six hours late (see the manifest), which is no live wind. So nothing
/// collects it, and the development mode does not run it. The sample is
/// handed over all the same: the server's tests hold every network that
/// has one to the contract of `venti_sources::testing::check_station_adapter`,
/// and a pack that one day names the network finds it ready.
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(ArpaLombardia::new())
        .with_sample(fixtures::evening_at(), fixtures::evening())
}

impl Default for ArpaLombardia {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for ArpaLombardia {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new("sensors", sensors_url(&ctx.area))]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        match request.tag.as_str() {
            "sensors" => parse_sensors(&self.manifest.id, body, ctx),
            tag if tag.starts_with("values/") => parse_values(&self.manifest.id, body),
            _ => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

/// A text field of a row.
fn text<'a>(row: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    row.get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// A number of a row, which the portal writes as text.
fn number(row: &serde_json::Value, name: &str) -> Option<f64> {
    text(row, name)
        .and_then(|text| text.parse::<f64>().ok())
        .filter(|n| n.is_finite())
}

/// A station as the sensors' list describes it.
struct StationEntry {
    name: String,
    lat: Option<f64>,
    lon: Option<f64>,
    elevation: Option<f64>,
    sensors: Vec<Sensor>,
}

fn parse_sensors(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    let rows: Vec<serde_json::Value> = serde_json::from_slice(body).map_err(|e| {
        SourceError::Malformed(format!("the list of sensors is not the expected JSON: {e}"))
    })?;

    let mut parsed = Parsed::default();
    if rows.len() >= SENSOR_LIMIT {
        parsed.skipped.push(Skipped::new(
            "the list of sensors",
            format!("the answer holds {SENSOR_LIMIT} sensors, the most asked for, and may be cut"),
        ));
    }
    let mut stations: BTreeMap<String, StationEntry> = BTreeMap::new();
    for row in &rows {
        let (Some(sensor), Some(station), Some(kind)) = (
            text(row, "idsensore"),
            text(row, "idstazione"),
            text(row, "tipologia"),
        ) else {
            parsed.skipped.push(Skipped::new(
                &row.to_string(),
                "a sensor without its id, station or kind",
            ));
            continue;
        };
        let Some(kind) = Kind::from_portal_name(kind) else {
            parsed.skipped.push(Skipped::new(
                sensor,
                format!("a sensor of a kind not taken: {kind}"),
            ));
            continue;
        };
        let unit = text(row, "unit_dimisura").unwrap_or("no unit");
        if unit != kind.unit() {
            parsed.skipped.push(Skipped::new(
                sensor,
                format!("a sensor in {unit}, where {} was expected", kind.unit()),
            ));
            continue;
        }
        let entry = stations
            .entry(station.to_owned())
            .or_insert_with(|| StationEntry {
                name: text(row, "nomestazione").unwrap_or(station).to_owned(),
                lat: number(row, "lat"),
                lon: number(row, "lng"),
                elevation: number(row, "quota"),
                sensors: Vec::new(),
            });
        entry.sensors.push(Sensor {
            id: sensor.to_owned(),
            station: station.to_owned(),
            kind,
        });
    }

    let mut asked: Vec<Vec<Sensor>> = Vec::new();
    for (code, entry) in stations {
        // A station without an anemometer is not taken.
        if !entry.sensors.iter().any(|s| s.kind == Kind::Speed) {
            continue;
        }
        let station =
            match (entry.lat, entry.lon) {
                (Some(lat), Some(lon)) => GeoPoint::new(lat, lon)
                    .map_err(|e| e.to_string())
                    .and_then(|position| {
                        let id = StationId::new(source.clone(), code.clone())
                            .map_err(|e| e.to_string())?;
                        Ok(Station {
                            id,
                            name: entry.name.clone(),
                            position,
                            elevation_m: entry.elevation,
                        })
                    }),
                _ => Err("a station without a position".to_owned()),
            };
        match station {
            Ok(station) if ctx.area.contains(station.position) => {
                parsed.stations.push(station);
                asked.push(entry.sensors);
            }
            Ok(_) => {}
            Err(reason) => parsed.skipped.push(Skipped::new(&code, reason)),
        }
    }
    parsed.follow_up = values_requests(&asked, ctx.now);
    Ok(parsed)
}

fn parse_values(source: &SourceId, body: &[u8]) -> Result<Parsed, SourceError> {
    let rows: Vec<serde_json::Value> = serde_json::from_slice(body).map_err(|e| {
        SourceError::Malformed(format!("the values are not the expected JSON: {e}"))
    })?;

    let mut parsed = Parsed::default();
    if rows.len() >= ROW_LIMIT {
        parsed.skipped.push(Skipped::new(
            "the values",
            format!("the answer holds {ROW_LIMIT} rows, the most asked for, and may be cut"),
        ));
    }
    for row in &rows {
        let sensor = text(row, "s").and_then(|s| s.split_once('/'));
        let station = sensor.and_then(|(station, letter)| {
            let kind = Kind::from_letter(letter)?;
            let id = StationId::new(source.clone(), station).ok()?;
            Some((id, kind))
        });
        let Some((station, kind)) = station else {
            parsed.skipped.push(Skipped::new(
                &row.to_string(),
                "a row without a sensor the run asked for",
            ));
            continue;
        };
        let date = text(row, "data");
        let what = || {
            format!(
                "{}/{} {}",
                station.local(),
                kind.letter(),
                date.unwrap_or("without a date")
            )
        };
        let Some(time) = date.and_then(parse_time) else {
            parsed
                .skipped
                .push(Skipped::new(&what(), "time stamp cannot be read"));
            continue;
        };
        let value = text(row, "valore");
        let Some(number) = value.and_then(|v| v.parse::<f64>().ok()) else {
            parsed.skipped.push(Skipped::new(
                &what(),
                format!("value is not a number: {}", value.unwrap_or("none")),
            ));
            continue;
        };
        // "-9999 = dato mancante".
        if number == -9999.0 {
            continue;
        }
        let state = text(row, "stato").unwrap_or("none");
        if !matches!(state, "VA" | "VV") {
            parsed.skipped.push(Skipped::new(
                &what(),
                format!("the provider marks the value as not valid ({state})"),
            ));
            continue;
        }
        let operator = text(row, "idoperatore").unwrap_or("none");
        let mut observation = Observation::empty(station.clone(), time);
        match (kind, operator) {
            (Kind::Speed, "1") => observation.wind_speed_ms = Some(number),
            (Kind::Speed, "3") => observation.wind_gust_ms = Some(number),
            (Kind::Direction, "1" | "3") if is_no_direction(number) => continue,
            (Kind::Direction, "1") => observation.wind_direction_deg = Some(number),
            (Kind::Direction, "3") => observation.gust_direction_deg = Some(number),
            (Kind::Temperature, "1") => observation.temperature_c = Some(number),
            (Kind::Humidity, "1") => observation.humidity_pct = Some(number),
            _ => {
                parsed.skipped.push(Skipped::new(
                    &what(),
                    format!("a value of an operator not taken: {operator}"),
                ));
                continue;
            }
        }
        parsed.observations.push(observation);
    }
    Ok(parsed)
}

/// Whether a direction is one of the portal's words for none: "888, 8888 =
/// direzione vento variabile", "777, 7777 = calma".
fn is_no_direction(value: f64) -> bool {
    [777.0, 7777.0, 888.0, 8888.0].contains(&value)
}

/// Reads a time of the values, `2026-11-22T07:20:00.000` in UTC+1, as UTC.
fn parse_time(text: &str) -> Option<Timestamp> {
    let local = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
    Some((local - Duration::hours(1)).and_utc())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn reads_the_portals_times_as_utc_plus_one() {
        let at = |day, hour, minute| {
            Utc.with_ymd_and_hms(2026, 11, day, hour, minute, 0)
                .unwrap()
        };
        assert_eq!(parse_time("2026-11-22T07:20:00.000"), Some(at(22, 6, 20)));
        assert_eq!(parse_time("2026-11-22T07:20:00"), Some(at(22, 6, 20)));
        // Ten past midnight by the portal's clock is the day before in UTC.
        assert_eq!(parse_time("2026-11-22T00:10:00.000"), Some(at(21, 23, 10)));
        assert_eq!(parse_time("22/11/2026 07:20"), None);
        assert_eq!(parse_time(""), None);
    }

    #[test]
    fn the_window_begins_seven_hours_back_on_the_ten_minutes() {
        let now = Utc.with_ymd_and_hms(2026, 11, 22, 8, 47, 31).unwrap();
        // 08:47 UTC is 09:47 by the portal's clock; seven hours before, on
        // the ten minutes, is 02:40.
        assert_eq!(since(now), "2026-11-22T02:40:00");
        // Across midnight.
        let now = Utc.with_ymd_and_hms(2026, 11, 22, 3, 5, 0).unwrap();
        assert_eq!(since(now), "2026-11-21T21:00:00");
    }

    #[test]
    fn the_query_is_written_as_the_portal_reads_it() {
        assert_eq!(
            encode("data >= 'x' |> é"),
            "data%20%3E=%20%27x%27%20%7C%3E%20%C3%A9"
        );
        let sensor = Sensor {
            id: "11811".to_owned(),
            station: "1202".to_owned(),
            kind: Kind::Speed,
        };
        assert_eq!(sensor.arm(), "idsensore='11811','1202/v'");
    }

    #[test]
    fn the_words_for_no_direction_are_no_direction() {
        for value in [777.0, 7777.0, 888.0, 8888.0] {
            assert!(is_no_direction(value));
        }
        assert!(!is_no_direction(0.0));
        assert!(!is_no_direction(360.0));
    }

    #[test]
    fn every_kind_has_its_letter_and_unit() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_letter(&kind.letter().to_string()), Some(kind));
            assert_eq!(Kind::from_portal_name(kind.portal_name()), Some(kind));
        }
        assert_eq!(Kind::from_letter("vv"), None);
        assert_eq!(Kind::from_letter(""), None);
        assert_eq!(Kind::Speed.unit(), "m/s");
    }
}
