//! Adapter for the automatic weather stations of ARSO, the Slovenian
//! Environment Agency (Agencija Republike Slovenije za okolje): the state's
//! network, from the coast to Kredarica below the Triglav at 2514 m.
//!
//! One file holds the newest values of every station: its position, height
//! and names, and what it measured over its last interval. A run asks it
//! once and keeps the stations inside the run's area.
//!
//! The file is XML and its element names are Slovenian abbreviations. What
//! each means is taken from the description that the file writes beside
//! every value, `<x_var_desc>` and `<x_var_unit>` (read 2026-11-22), and
//! from ARSO's description of its XML (version 0.80 of 2009,
//! `https://meteo.arso.gov.si/uploads/meteo/help/sl/xml_service.html`):
//!
//! | Element | ARSO's description | Unit | Taken as |
//! |---|---|---|---|
//! | `ffavg_val` | Povprečna hitrost vetra v časovnem intervalu (mean wind speed in the interval) | m/s | wind speed |
//! | `ffmax_val` | Maksimalna hitrost vetra v časovnem intervalu (strongest wind in the interval); "hitrost vetra v sunkih" (speed in gusts) in the description | m/s | gust |
//! | `ddavg_val` | Povprečna smer vetra v časovnem intervalu (mean direction in the interval) | ° | wind direction |
//! | `ddmax_val` | Smer največjega sunka vetra v časovnem intervalu (direction of the strongest gust) | ° | gust direction |
//! | `t` | Temperatura | °C | temperature |
//! | `rh` | Relativna vlažnost | % | humidity |
//! | `p` | Zračni tlak na lokaciji (air pressure at the place) | hPa | pressure |
//! | `valid_UTC` | datum in ura veljavnosti podatkov v UTC (the values' time in UTC), the end of the interval | | the reading's time |
//! | `domain_meteosiId` | the station's code, "unique" and not expected to change | | the station's id |
//! | `domain_lat`, `domain_lon` | position, "as a rule WGS84" | ° | position |
//! | `domain_altitude` | nadmorska višina (height above sea level) | m | height |
//! | `domain_longTitle` | the longer descriptive name, with Slovenian letters | | name |
//!
//! Not taken: `ff_val` and `dd_val` (the wind "speed" and "direction"
//! without an interval: equal to the means at the ten-minute stations, the
//! newest ten minutes at the half-hourly ones), `msl` (the pressure reduced
//! to sea level, which the canonical record has no field for), the dew
//! point, the means of temperature and humidity, rain, snow, radiation,
//! soil temperatures, clouds and visibility.
//!
//! **Times.** The file writes the time of a value three ways: `valid` in
//! local time with the zone's name ("22.11.2026 8:50 CET"; in summer
//! presumably "CEST", not seen), `valid_UTC` ("22.11.2026 7:50 UTC") and,
//! for the time it was issued, RFC 822 with an offset. Only `valid_UTC` is
//! read, and only when it ends in " UTC": a local time is never taken for
//! UTC. The intervals differ by station: ten minutes at most stations, half
//! an hour at the main ones (`validStart`, `validEnd`).
//!
//! **Units.** Each value's unit is in the file beside it. A reading whose
//! unit is not the one the adapter takes is skipped and reported, so that
//! a change of unit cannot pass as a value. All of them are canonical
//! already: nothing is converted.
//!
//! **States.** Every value has a `<x_state>`, which no description found
//! explains. On 2026-11-22 it was 0 beside every value of wind,
//! temperature, humidity and pressure, and 2 beside some empty ones. A
//! value is taken when its state is 0 or empty and left out otherwise.
//!
//! A station without a reading of anything taken has a station and no
//! observation.

pub mod fixtures;
mod xml;

use std::collections::HashMap;

use chrono::{NaiveDateTime, TimeZone, Utc};
use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

/// The newest values of every automatic station.
const LATEST: &str = "https://meteo.arso.gov.si/uploads/probase/www/observ/surface/text/sl/observationAms_si_latest.xml";

/// The address of the newest values of every automatic station.
pub fn latest_url() -> String {
    LATEST.to_owned()
}

/// ARSO's automatic weather stations as an observation source.
#[derive(Debug, Clone)]
pub struct Arso {
    manifest: Manifest,
}

impl Arso {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-arso is valid");
        Self { manifest }
    }
}

/// The network as a build takes it in: the adapter, and the sample that
/// answers for it in the server's development mode. The server's list of
/// adapters has one line for this function and names nothing else of the
/// crate.
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(Arso::new())
        .with_sample(fixtures::evening_at(), fixtures::evening())
}

impl Default for Arso {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for Arso {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        // One file holds every station; there is no smaller answer for an
        // area.
        vec![Request::new("latest", latest_url())]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        match request.tag.as_str() {
            "latest" => parse_latest(&self.manifest.id, body, ctx),
            _ => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

/// A value that is read, the unit ARSO must give for it, and where it goes.
struct Field {
    /// The element of the value.
    value: &'static str,
    /// The element of its state.
    state: &'static str,
    /// The element of its unit.
    unit: &'static str,
    /// The unit the value is taken in.
    expected_unit: &'static str,
    /// Where the value goes in the canonical record.
    set: fn(&mut Observation, f64),
}

const FIELDS: [Field; 7] = [
    Field {
        value: "ffavg_val",
        state: "ffavg_state",
        unit: "ffavg_var_unit",
        expected_unit: "m/s",
        set: |o, v| o.wind_speed_ms = Some(v),
    },
    Field {
        value: "ffmax_val",
        state: "ffmax_state",
        unit: "ffmax_var_unit",
        expected_unit: "m/s",
        set: |o, v| o.wind_gust_ms = Some(v),
    },
    Field {
        value: "ddavg_val",
        state: "ddavg_state",
        unit: "ddavg_var_unit",
        expected_unit: "°",
        set: |o, v| o.wind_direction_deg = Some(v),
    },
    Field {
        value: "ddmax_val",
        state: "ddmax_state",
        unit: "ddmax_var_unit",
        expected_unit: "°",
        set: |o, v| o.gust_direction_deg = Some(v),
    },
    Field {
        value: "t",
        state: "t_state",
        unit: "t_var_unit",
        expected_unit: "°C",
        set: |o, v| o.temperature_c = Some(v),
    },
    Field {
        value: "rh",
        state: "rh_state",
        unit: "rh_var_unit",
        expected_unit: "%",
        set: |o, v| o.humidity_pct = Some(v),
    },
    Field {
        value: "p",
        state: "p_state",
        unit: "p_var_unit",
        expected_unit: "hPa",
        set: |o, v| o.pressure_hpa = Some(v),
    },
];

/// The leaf values of one station's `<metData>`, by element name. Where a
/// name is written more than once (the webcams), the first is kept.
struct Values<'a>(HashMap<&'a str, &'a str>);

impl<'a> Values<'a> {
    fn of(element: &'a xml::Element) -> Self {
        let mut map = HashMap::new();
        for child in &element.children {
            map.entry(child.name.as_str())
                .or_insert_with(|| child.text.trim());
        }
        Self(map)
    }

    /// The text of an element, if it is there and not empty.
    fn get(&self, name: &str) -> Option<&'a str> {
        self.0.get(name).copied().filter(|text| !text.is_empty())
    }
}

fn parse_latest(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    let text = std::str::from_utf8(body)
        .map_err(|e| SourceError::Malformed(format!("the file is not UTF-8: {e}")))?;
    let root = xml::parse(text).map_err(|e| {
        SourceError::Malformed(format!("the file is not XML that can be read: {e}"))
    })?;
    if root.name != "data" {
        return Err(SourceError::Malformed(format!(
            "the file's root element is <{}>, not <data>",
            root.name
        )));
    }

    let mut parsed = Parsed::default();
    for (index, element) in root
        .children
        .iter()
        .filter(|c| c.name == "metData")
        .enumerate()
    {
        let values = Values::of(element);
        let Some(code) = values.get("domain_meteosiId") else {
            let what = values
                .get("domain_title")
                .map_or_else(|| format!("metData {}", index + 1), str::to_owned);
            parsed
                .skipped
                .push(Skipped::new(&what, "a station without an id"));
            continue;
        };
        let position = match (
            values.get("domain_lat").map(str::parse::<f64>),
            values.get("domain_lon").map(str::parse::<f64>),
        ) {
            (Some(Ok(lat)), Some(Ok(lon))) => GeoPoint::new(lat, lon).map_err(|e| e.to_string()),
            _ => Err("a station without a position".to_owned()),
        };
        let station = position.and_then(|position| {
            let id = StationId::new(source.clone(), code).map_err(|e| e.to_string())?;
            let name = values
                .get("domain_longTitle")
                .or_else(|| values.get("domain_shortTitle"))
                .or_else(|| values.get("domain_title"))
                .unwrap_or(code);
            Ok(Station {
                id,
                name: name.to_owned(),
                position,
                // A height that is missing or cannot be read is left empty.
                elevation_m: values
                    .get("domain_altitude")
                    .and_then(|text| text.parse::<f64>().ok()),
            })
        });
        let station = match station {
            Ok(station) => station,
            Err(reason) => {
                parsed.skipped.push(Skipped::new(code, reason));
                continue;
            }
        };
        if !ctx.area.contains(station.position) {
            continue;
        }

        match reading(&station, &values) {
            Ok(Some(observation)) => parsed.observations.push(observation),
            Ok(None) => {}
            Err(skipped) => parsed.skipped.push(skipped),
        }
        parsed.stations.push(station);
    }
    Ok(parsed)
}

/// The observation of one station, none when it measured nothing that is
/// taken, or what was skipped and why.
fn reading(station: &Station, values: &Values<'_>) -> Result<Option<Observation>, Skipped> {
    let code = station.id.local();
    let mut observation: Option<Observation> = None;
    for field in &FIELDS {
        let Some(text) = values.get(field.value) else {
            continue;
        };
        if values.get(field.state).is_some_and(|state| state != "0") {
            continue;
        }
        if let Some(unit) = values.get(field.unit)
            && unit != field.expected_unit
        {
            return Err(Skipped::new(
                &format!("{code} {}", field.value),
                format!(
                    "the unit is {unit}, not {}: the reading is not taken",
                    field.expected_unit
                ),
            ));
        }
        let Ok(value) = text.parse::<f64>() else {
            return Err(Skipped::new(
                &format!("{code} {}", field.value),
                format!("value cannot be read: {text}"),
            ));
        };
        let observation = match observation.as_mut() {
            Some(observation) => observation,
            None => {
                let date = values.get("valid_UTC").unwrap_or("");
                let Some(time) = parse_utc(date) else {
                    return Err(Skipped::new(
                        &format!("{code} {date}"),
                        "time stamp cannot be read",
                    ));
                };
                observation.insert(Observation::empty(station.id.clone(), time))
            }
        };
        (field.set)(observation, value);
    }
    Ok(observation)
}

/// Reads the time of a value as the file writes it in UTC:
/// `22.11.2026 7:50 UTC`. A time in any other zone, `22.11.2026 8:50 CET`,
/// is not read.
fn parse_utc(text: &str) -> Option<Timestamp> {
    let utc = text.trim().strip_suffix(" UTC")?;
    let naive = NaiveDateTime::parse_from_str(utc, "%d.%m.%Y %H:%M").ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_files_utc_times_and_no_local_ones() {
        let at = |d, h, m| Utc.with_ymd_and_hms(2026, 11, d, h, m, 0).unwrap();
        assert_eq!(parse_utc("22.11.2026 7:50 UTC"), Some(at(22, 7, 50)));
        assert_eq!(parse_utc("22.11.2026 07:50 UTC"), Some(at(22, 7, 50)));
        assert_eq!(parse_utc("18.11.2026 18:50 UTC"), Some(at(18, 18, 50)));
        assert_eq!(parse_utc("1.11.2026 0:00 UTC"), Some(at(1, 0, 0)));
        assert_eq!(
            parse_utc("31.12.2026 23:50 UTC"),
            Some(Utc.with_ymd_and_hms(2026, 12, 31, 23, 50, 0).unwrap())
        );
        // A summer time, written in UTC, is read as it is: 14:00 CEST is
        // 12:00 UTC, and the file says 12:00 UTC.
        assert_eq!(
            parse_utc("1.7.2026 12:00 UTC"),
            Some(Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap())
        );
        // Local times are never taken for UTC.
        assert_eq!(parse_utc("22.11.2026 8:50 CET"), None);
        assert_eq!(parse_utc("1.7.2026 14:00 CEST"), None);
        assert_eq!(parse_utc("22.11.2026 8:50"), None);
        assert_eq!(parse_utc("22 Nov 2026 07:50:00 +0000"), None);
        assert_eq!(parse_utc("32.11.2026 7:50 UTC"), None);
        assert_eq!(parse_utc("22.11.2026 24:00 UTC"), None);
        assert_eq!(parse_utc(""), None);
    }

    /// The units the adapter takes are the ones the recorded file gives
    /// beside each value, for every station, and the descriptions say what
    /// the module's table says.
    #[test]
    fn the_units_and_meanings_are_the_files_own() {
        let text = include_str!("../tests/fixtures/observationAms_si_latest.xml");
        let root = xml::parse(text).unwrap();
        let expected = [
            (
                "ffavg",
                "Povprečna hitrost vetra v časovnem intervalu",
                "m/s",
            ),
            (
                "ffmax",
                "Maksimalna hitrost vetra v časovnem intervalu",
                "m/s",
            ),
            ("ddavg", "Povprečna smer vetra v časovnem intervalu", "°"),
            (
                "ddmax",
                "Smer največjega sunka vetra v časovnem intervalu",
                "°",
            ),
            ("t", "Temperatura", "°C"),
            ("rh", "Relativna vlažnost", "%"),
            ("p", "Zračni tlak na lokaciji", "hPa"),
            ("msl", "Zračni tlak reduciran na morski nivo", "hPa"),
        ];
        let mut stations = 0;
        for element in root.children.iter().filter(|c| c.name == "metData") {
            stations += 1;
            let values = Values::of(element);
            for (name, description, unit) in expected {
                assert_eq!(
                    values.get(&format!("{name}_var_desc")),
                    Some(description),
                    "{name}"
                );
                assert_eq!(
                    values.get(&format!("{name}_var_unit")),
                    Some(unit),
                    "{name}"
                );
            }
            for field in &FIELDS {
                assert_eq!(values.get(field.unit), Some(field.expected_unit));
            }
        }
        assert_eq!(stations, 15);
    }
}
