//! Adapter for the weather stations of the Autonomous Province of Trento,
//! which its weather service, Meteotrentino, runs and publishes as open
//! data.
//!
//! The province's service answers for one station at a time. A run asks
//! the list of stations once, and then the values of each station of the
//! list that measures wind and stands in the run's area, one request each.
//!
//! Which stations measure wind the list does not say. One pass over the
//! answers of all 113 stations in service, on 2026-11-22, found 46 with an
//! anemometer: they are [`WIND_STATIONS`]. The others measure temperature,
//! rain or snow and are not asked: asking them too would cost half as much
//! again, for no wind. A station put into service after that pass is asked
//! as well, since nobody knows yet what it measures. Two things keep the
//! list honest, and both show in the run's report, run after run: a
//! station of the list whose answer holds no wind, and one that the
//! station list says is out of service.
//!
//! A station's answer holds its values since midnight at the start of the
//! day before, every quarter of an hour: temperature, rain, wind, global
//! radiation and relative humidity, each in a section of its own with the
//! unit written on every row. The wind, the temperature and the humidity
//! are read; the rows of one time make one observation. The units are
//! checked: a row in another unit than the documented one is reported and
//! left out. The answer has no pressure.
//!
//! The times are written without a zone. They are "ora solare", UTC+1 the
//! whole year: the province's note on its recent data says so, and on
//! 2026-11-22 the service's other answer for a station, which writes the
//! zone (`+01`), had the same times for the same values. That this holds in
//! summer too is to verify. A wind row of the full or the half hour is the
//! ten-minute value of that time as the other answer gives it; the rows of
//! a quarter past and a quarter to are not, and what they are is to
//! verify. A time is taken as the end of what it measures, as for the other
//! sources.
//!
//! The answers are XML, which a small reader of the crate's own reads: the
//! workspace has no XML library.

pub mod fixtures;
mod xml;

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

use crate::xml::Element;

const SERVICE: &str = "https://dati.meteotrentino.it/service.asmx";

/// The stations that had an anemometer on [`WIND_STATIONS_DATE`]: every
/// station in service whose answer held wind on that day.
pub const WIND_STATIONS: [&str; 46] = [
    "T0010", "T0065", "T0066", "T0071", "T0074", "T0076", "T0103", "T0104", "T0118", "T0129",
    "T0135", "T0146", "T0147", "T0153", "T0175", "T0179", "T0193", "T0354", "T0355", "T0357",
    "T0360", "T0366", "T0367", "T0368", "T0369", "T0372", "T0373", "T0374", "T0379", "T0382",
    "T0384", "T0403", "T0404", "T0406", "T0409", "T0414", "T0417", "T0419", "T0420", "T0421",
    "T0423", "T0425", "T0433", "T0437", "T0450", "T0469",
];

/// The day [`WIND_STATIONS`] was made, as year, month and day. A station
/// put into service later is asked too.
pub const WIND_STATIONS_DATE: (i32, u32, u32) = (2026, 11, 22);

/// The tag of the station list's request.
const STATIONS: &str = "stations";
/// The prefix of the tag of a station's request; the station's code follows.
const VALUES: &str = "values/";

/// A quantity of a station's answer: its section, the name of its rows,
/// and what is read of a row.
struct Quantity {
    section: &'static str,
    row: &'static str,
    fields: &'static [Field],
}

/// A value of a row: its element, the attribute that names its unit, the
/// unit the service documents, and the observation field it fills.
struct Field {
    element: &'static str,
    unit_attribute: &'static str,
    unit: &'static str,
    set: fn(&mut Observation, f64),
}

const WIND: Quantity = Quantity {
    section: "venti",
    row: "vento_al_suolo",
    fields: &[
        Field {
            element: "v",
            unit_attribute: "UM_VV",
            unit: "m/s",
            set: |o, v| o.wind_speed_ms = Some(v),
        },
        Field {
            element: "vmax",
            unit_attribute: "UM_VVMAX",
            unit: "m/s",
            set: |o, v| o.wind_gust_ms = Some(v),
        },
        // Degrees from north ("gradi Nord"), the direction the wind comes
        // from, as a weather service gives it. To verify: the service does
        // not say.
        Field {
            element: "d",
            unit_attribute: "UM_DV",
            unit: "gN",
            set: |o, v| o.wind_direction_deg = Some(v),
        },
    ],
};

const TEMPERATURE: Quantity = Quantity {
    section: "temperature",
    row: "temperatura_aria",
    fields: &[Field {
        element: "temperatura",
        unit_attribute: "UM",
        unit: "°C",
        set: |o, v| o.temperature_c = Some(v),
    }],
};

const HUMIDITY: Quantity = Quantity {
    section: "umidita_relativa",
    row: "umidita_relativa",
    fields: &[Field {
        element: "rh",
        unit_attribute: "UM",
        unit: "%",
        set: |o, v| o.humidity_pct = Some(v),
    }],
};

const QUANTITIES: [&Quantity; 3] = [&WIND, &TEMPERATURE, &HUMIDITY];

/// The address of the list of stations.
pub fn stations_url() -> String {
    format!("{SERVICE}/listaStazioni")
}

/// The address of a station's values since midnight at the start of the
/// day before.
pub fn values_url(code: &str) -> String {
    format!("{SERVICE}/ultimiDatiStazione?codice={code}")
}

/// The weather stations of Trentino as an observation source.
#[derive(Debug, Clone)]
pub struct Meteotrentino {
    manifest: Manifest,
}

impl Meteotrentino {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-meteotrentino is valid");
        Self { manifest }
    }
}

/// The network as a build takes it in: the adapter, and the sample that
/// answers for it in the server's development mode. The server's list of
/// adapters has one line for this function and names nothing else of the
/// crate.
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(Meteotrentino::new())
        .with_sample(fixtures::evening_at(), fixtures::evening())
}

impl Default for Meteotrentino {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for Meteotrentino {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        // Which stations to ask is known only from the list: they follow
        // as the list's follow-up requests.
        vec![Request::new(STATIONS, stations_url())]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        if request.tag == STATIONS {
            return parse_stations(&self.manifest.id, body, ctx);
        }
        match request.tag.strip_prefix(VALUES) {
            Some(code) => parse_values(&self.manifest.id, code, body),
            None => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

/// Reads a whole answer as an XML document whose root has the given name.
fn document(body: &[u8], root: &str, what: &str) -> Result<Element, SourceError> {
    let text = std::str::from_utf8(body)
        .map_err(|e| SourceError::Malformed(format!("{what} is not UTF-8: {e}")))?;
    let document = xml::parse(text)
        .map_err(|e| SourceError::Malformed(format!("{what} is not the expected XML: {e}")))?;
    if document.name != root {
        return Err(SourceError::Malformed(format!(
            "{what} is <{}>, not <{root}>",
            document.name
        )));
    }
    Ok(document)
}

/// Whether a code can stand in an address as it is. The service's codes
/// are a letter and four digits.
fn is_plain_code(code: &str) -> bool {
    !code.is_empty() && code.len() <= 16 && code.chars().all(|c| c.is_ascii_alphanumeric())
}

fn parse_stations(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    let list = document(body, "ArrayOfAnagrafica", "the station list")?;
    let (year, month, day) = WIND_STATIONS_DATE;
    let list_made = NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| SourceError::Malformed("the wind stations' date is no date".to_owned()))?;

    let mut parsed = Parsed::default();
    for entry in list.children_named("anagrafica") {
        let code = entry.text_of("codice").unwrap_or_default();
        // A station with an end date is out of service; the list keeps
        // such stations for their history. One of the adapter's wind
        // stations among them is said.
        let end = entry.text_of("fine").unwrap_or_default();
        if !end.is_empty() {
            if WIND_STATIONS.contains(&code) {
                parsed.skipped.push(Skipped::new(
                    code,
                    format!(
                        "a station of the adapter's list of wind stations that the station \
                         list has out of service since {end}"
                    ),
                ));
            }
            continue;
        }
        if !is_plain_code(code) {
            parsed.skipped.push(Skipped::new(
                &format!("station {code:?}"),
                "a station without a code that can be asked for",
            ));
            continue;
        }
        let begun = entry
            .text_of("inizio")
            .and_then(|text| NaiveDate::parse_from_str(text, "%d/%m/%Y").ok());
        let new_since_the_list = begun.is_some_and(|begun| begun > list_made);
        if !WIND_STATIONS.contains(&code) && !new_since_the_list {
            continue;
        }
        match read_station(source, code, entry) {
            Ok(station) if ctx.area.contains(station.position) => {
                parsed
                    .follow_up
                    .push(Request::new(format!("{VALUES}{code}"), values_url(code)));
                parsed.stations.push(station);
            }
            Ok(_) => {}
            Err(reason) => parsed.skipped.push(Skipped::new(code, reason)),
        }
    }
    Ok(parsed)
}

/// What is read of a station: its name, its position in degrees and its
/// height. The list gives the position in UTM as well; the degrees are
/// taken.
fn read_station(source: &SourceId, code: &str, entry: &Element) -> Result<Station, String> {
    let number = |name: &str| -> Result<f64, String> {
        let text = entry.text_of(name).unwrap_or_default();
        text.parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("station cannot be read: {name} is {text:?}"))
    };
    let position = GeoPoint::new(number("latitudine")?, number("longitudine")?)
        .map_err(|e| format!("station cannot be read: {e}"))?;
    let name = entry.text_of("nome").unwrap_or_default();
    if name.is_empty() {
        return Err("station cannot be read: it has no name".to_owned());
    }
    // A height that is missing or not a number is no height; the station
    // and its wind are kept.
    let elevation_m = entry
        .text_of("quota")
        .and_then(|text| text.parse::<f64>().ok())
        .filter(|value| value.is_finite());
    Ok(Station {
        id: StationId::new(source.clone(), code.to_owned()).map_err(|e| e.to_string())?,
        name: name.to_owned(),
        position,
        elevation_m,
    })
}

fn parse_values(source: &SourceId, code: &str, body: &[u8]) -> Result<Parsed, SourceError> {
    let answer = document(body, "datiOggi", &format!("the answer for {code}"))?;
    let station = StationId::new(source.clone(), code.to_owned())
        .map_err(|e| SourceError::Malformed(format!("the station {code:?}: {e}")))?;

    let mut parsed = Parsed::default();
    let mut by_time: BTreeMap<Timestamp, Observation> = BTreeMap::new();
    let mut wind_rows = 0;
    for quantity in QUANTITIES {
        let Some(section) = answer.child(quantity.section) else {
            continue;
        };
        for row in section.children_named(quantity.row) {
            let date = row.text_of("data").unwrap_or_default();
            let what = format!("{code} {} {date}", quantity.row);
            let values = match read_row(quantity, row) {
                Ok(values) if values.is_empty() => continue,
                Ok(values) => values,
                Err(reason) => {
                    parsed.skipped.push(Skipped::new(&what, reason));
                    continue;
                }
            };
            let Some(time) = parse_time(date) else {
                parsed
                    .skipped
                    .push(Skipped::new(&what, "time stamp cannot be read"));
                continue;
            };
            if quantity.section == WIND.section {
                wind_rows += 1;
            }
            let observation = by_time
                .entry(time)
                .or_insert_with(|| Observation::empty(station.clone(), time));
            for (field, value) in values {
                (field.set)(observation, value);
            }
        }
    }
    if wind_rows == 0 {
        parsed.skipped.push(Skipped::new(
            code,
            "the station's answer holds no wind: the anemometer may be out of order, or the \
             station no longer belong on the adapter's list of wind stations",
        ));
    }
    parsed.observations.extend(by_time.into_values());
    Ok(parsed)
}

/// The values of one row, each with its field. A value that is missing or
/// empty is no value; a row with a value that is not a number, or in
/// another unit than the documented one, is not read at all.
fn read_row(
    quantity: &'static Quantity,
    row: &Element,
) -> Result<Vec<(&'static Field, f64)>, String> {
    let mut values = Vec::new();
    for field in quantity.fields {
        let text = row.text_of(field.element).unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        let unit = row.attribute(field.unit_attribute).unwrap_or_default();
        if unit != field.unit {
            return Err(format!(
                "the unit of {} is {unit:?}, not {:?}",
                field.element, field.unit
            ));
        }
        let value = text
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("value cannot be read: {} is {text:?}", field.element))?;
        values.push((field, value));
    }
    Ok(values)
}

/// The service's time stamps are "ora solare", one hour ahead of UTC the
/// whole year: see the crate's documentation.
const HOURS_AHEAD_OF_UTC: i64 = 1;

/// Reads a time stamp of the service, `2026-11-22T09:00:00`.
fn parse_time(text: &str) -> Option<Timestamp> {
    let local = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").ok()?;
    Some(Utc.from_utc_datetime(&(local - Duration::hours(HOURS_AHEAD_OF_UTC))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_services_time_as_one_hour_ahead_of_utc() {
        let at = |month, day, hour, minute| {
            Utc.with_ymd_and_hms(2026, month, day, hour, minute, 0)
                .unwrap()
        };
        assert_eq!(parse_time("2026-11-22T09:00:00"), Some(at(11, 22, 8, 0)));
        // Midnight is the evening before in UTC.
        assert_eq!(parse_time("2026-11-21T00:00:00"), Some(at(11, 20, 23, 0)));
        // In summer too, by the province's note: to verify.
        assert_eq!(parse_time("2026-07-15T14:15:00"), Some(at(7, 15, 13, 15)));
        assert_eq!(parse_time("2026-11-22T09:00"), None);
        assert_eq!(parse_time("2026-11-22 09:00:00"), None);
        assert_eq!(parse_time("2026-11-22T09:00:00+01"), None);
        assert_eq!(parse_time("22/11/2026 09:00"), None);
        assert_eq!(parse_time(""), None);
    }

    #[test]
    fn the_list_of_wind_stations_is_sorted_and_plain() {
        assert!(WIND_STATIONS.windows(2).all(|w| w[0] < w[1]));
        assert!(WIND_STATIONS.iter().all(|code| is_plain_code(code)));
        let (year, month, day) = WIND_STATIONS_DATE;
        assert!(NaiveDate::from_ymd_opt(year, month, day).is_some());
    }

    #[test]
    fn only_a_plain_code_goes_into_an_address() {
        assert!(is_plain_code("T0129"));
        for code in [
            "",
            "T0129&h=1",
            "T 0129",
            "T0129/..",
            "Tò129",
            "T012345678901234567",
        ] {
            assert!(!is_plain_code(code), "{code}");
        }
    }

    fn row(quantity: &'static Quantity, attributes: &str, body: &str) -> Result<Vec<f64>, String> {
        let row = xml::parse(&format!("<r {attributes}>{body}</r>")).unwrap();
        read_row(quantity, &row).map(|values| values.into_iter().map(|(_, v)| v).collect())
    }

    #[test]
    fn reads_a_row_in_the_documented_units_and_no_other() {
        let units = r#"UM_VV="m/s" UM_VVMAX="m/s" UM_DV="gN""#;
        assert_eq!(
            row(&WIND, units, "<v>1.9</v><vmax>5.2</vmax><d>46</d>"),
            Ok(vec![1.9, 5.2, 46.0])
        );
        // A missing or empty value is none; the rest of the row is read.
        assert_eq!(
            row(&WIND, units, "<v>1.9</v><vmax /><d></d>"),
            Ok(vec![1.9])
        );
        assert_eq!(row(&WIND, units, "<data>x</data>"), Ok(vec![]));
        assert_eq!(
            row(
                &WIND,
                r#"UM_VV="km/h" UM_VVMAX="m/s" UM_DV="gN""#,
                "<v>7</v>"
            ),
            Err("the unit of v is \"km/h\", not \"m/s\"".to_owned())
        );
        // A unit that is not written is not the documented one.
        assert_eq!(
            row(&WIND, "", "<v>7</v>"),
            Err("the unit of v is \"\", not \"m/s\"".to_owned())
        );
        for bad in ["n/d", "1,9", "NaN", "inf"] {
            assert_eq!(
                row(&WIND, units, &format!("<v>{bad}</v>")),
                Err(format!("value cannot be read: v is {bad:?}"))
            );
        }
        assert_eq!(
            row(
                &TEMPERATURE,
                r#"UM="°C""#,
                "<temperatura>-7.8</temperatura>"
            ),
            Ok(vec![-7.8])
        );
        assert_eq!(row(&HUMIDITY, r#"UM="%""#, "<rh>94</rh>"), Ok(vec![94.0]));
        assert_eq!(
            row(&HUMIDITY, r#"UM="‰""#, "<rh>940</rh>"),
            Err("the unit of rh is \"‰\", not \"%\"".to_owned())
        );
    }
}
