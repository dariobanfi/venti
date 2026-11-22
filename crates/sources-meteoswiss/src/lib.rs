//! Adapter for SwissMetNet, the automatic weather stations of the Federal
//! Office of Meteorology and Climatology MeteoSwiss.
//!
//! A run asks two files of MeteoSwiss's open data, each once: the newest
//! ten-minute values of every station in one file, `VQHA80.csv`, and the
//! list of the stations with their names, heights and positions,
//! `ogd-smn_meta_stations.csv`. Both are text with a semicolon between
//! fields and a line a station. The values have no position, so they are
//! nothing without the list.
//!
//! What a column of the values means and in which unit it is, is taken from
//! MeteoSwiss's own description of its parameters, the file
//! `ogd-smn_meta_parameters.csv`, and from the documentation of the
//! automatic weather stations, which names the columns of `VQHA80.csv` and
//! says that its times are UTC
//! (<https://opendatadocs.meteoswiss.ch/a-data-groundbased/a1-automatic-weather-stations>,
//! read 2026-11-22). The rows of the description for the six parameters read
//! here are in `tests/fixtures`, and a test holds the units below against
//! them:
//!
//! - `fu3010z0`, "Wind speed; ten minutes mean in km/h";
//! - `fu3010z1`, "Gust peak (one second); maximum in km/h";
//! - `dkl010z0`, "Wind direction; ten minutes mean", in degrees;
//! - `tre200s0`, "Air temperature 2 m above ground; current value", °C;
//! - `ure200s0`, "Relative air humidity 2 m above ground; current value", %;
//! - `prestas0`, "Atmospheric pressure at barometric altitude (QFE); current
//!   value", hPa. That is the pressure at the station, a few metres from the
//!   station's height at most (the list gives both heights), and it is the
//!   one taken. The two pressures reduced to sea level are not.
//!
//! A `-` stands for no value. Four stations measure the wind on a tower,
//! 150 m to 230 m above the ground in the documentation's words, in columns
//! of their own (`fu3towz0` and the others with `tow`): that is not the wind
//! at the station's height, and it is not taken. Those stations have no wind
//! at the ground in the file.
//!
//! The documentation does not say whether a time marks the start or the end
//! of its ten minutes. It is treated as the end, as for the other sources.
//! To verify.
//!
//! The list is written in ISO 8859-1, not in UTF-8; the values file is
//! ASCII. A file that is valid UTF-8 is read as UTF-8, so the adapter keeps
//! working should MeteoSwiss change the list's encoding.
//!
//! The list gives every position twice: in the Swiss coordinates LV95 and in
//! WGS84 degrees. The degrees are taken. A test holds them against the LV95
//! coordinates by swisstopo's approximate formulas.
//!
//! The values are of every station, and the file takes no list of stations
//! to answer for. The values of a station outside the run's area reach the
//! pipeline, which leaves them out and says so.

pub mod fixtures;

use std::collections::HashMap;

use chrono::{NaiveDateTime, TimeZone, Utc};
use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

/// The file of the newest values of every station.
const VALUES: &str = "https://data.geo.admin.ch/ch.meteoschweiz.messwerte-aktuell/VQHA80.csv";

/// The list of the stations.
const STATIONS: &str =
    "https://data.geo.admin.ch/ch.meteoschweiz.ogd-smn/ogd-smn_meta_stations.csv";

/// A box around Switzerland and Liechtenstein, with some room, as south,
/// west, north and east: the stations of the list lay between 45.84° and
/// 47.70° N and 6.10° and 10.43° E on 2026-11-22. An area that does not
/// reach into it has no station of the network, and nothing is asked for
/// it.
const COVERAGE: (f64, f64, f64, f64) = (45.6, 5.8, 47.95, 10.7);

/// A column of the values and the observation field it fills.
struct Parameter {
    /// MeteoSwiss's code for the parameter, the column's name.
    code: &'static str,
    /// The unit MeteoSwiss's description of its parameters gives for it.
    /// The values file names no unit; a test holds this one against the
    /// description.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the test of the units"))]
    unit: &'static str,
    set: fn(&mut Observation, f64),
}

const PARAMETERS: [Parameter; 6] = [
    Parameter {
        code: "fu3010z0",
        unit: "km/h",
        set: |o, v| o.wind_speed_ms = Some(metres_per_second(v)),
    },
    Parameter {
        code: "fu3010z1",
        unit: "km/h",
        set: |o, v| o.wind_gust_ms = Some(metres_per_second(v)),
    },
    Parameter {
        code: "dkl010z0",
        unit: "°",
        set: |o, v| o.wind_direction_deg = Some(v),
    },
    Parameter {
        code: "tre200s0",
        unit: "°C",
        set: |o, v| o.temperature_c = Some(v),
    },
    Parameter {
        code: "ure200s0",
        unit: "%",
        set: |o, v| o.humidity_pct = Some(v),
    },
    Parameter {
        code: "prestas0",
        unit: "hPa",
        set: |o, v| o.pressure_hpa = Some(v),
    },
];

/// Kilometres an hour as metres per second, to a hundredth: the file gives
/// a tenth of a km/h, which is 0.028 m/s.
fn metres_per_second(kilometres_an_hour: f64) -> f64 {
    (kilometres_an_hour / 3.6 * 100.0).round() / 100.0
}

/// The address of the newest values of every station.
pub fn values_url() -> String {
    VALUES.to_owned()
}

/// The address of the list of stations.
pub fn stations_url() -> String {
    STATIONS.to_owned()
}

/// SwissMetNet as an observation source.
#[derive(Debug, Clone)]
pub struct MeteoSwissSmn {
    manifest: Manifest,
}

impl MeteoSwissSmn {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-meteoswiss is valid");
        Self { manifest }
    }
}

/// The network as a build takes it in: the adapter, and the sample that
/// answers for it in the server's development mode. The server's list of
/// adapters has one line for this function and names nothing else of the
/// crate.
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(MeteoSwissSmn::new())
        .with_sample(fixtures::evening_at(), fixtures::evening())
}

impl Default for MeteoSwissSmn {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for MeteoSwissSmn {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, ctx: &PlanCtx) -> Vec<Request> {
        let (south, west, north, east) = COVERAGE;
        let area = &ctx.area;
        if area.south() > north || area.north() < south || area.west() > east || area.east() < west
        {
            return Vec::new();
        }
        vec![
            Request::new("stations", stations_url()),
            Request::new("values", values_url()),
        ]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        match request.tag.as_str() {
            "stations" => parse_stations(&self.manifest.id, body, ctx),
            "values" => parse_values(&self.manifest.id, body),
            _ => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

/// A file of MeteoSwiss: its header's columns and its rows, each as the
/// line and its fields.
struct Table {
    columns: HashMap<String, usize>,
    width: usize,
    rows: Vec<(String, Vec<String>)>,
}

impl Table {
    fn read(body: &[u8], what: &str) -> Result<Self, SourceError> {
        let text = decode(body);
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let mut lines = text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .filter(|line| !line.trim().is_empty());
        let header = lines
            .next()
            .ok_or_else(|| SourceError::Malformed(format!("{what} is empty")))?;
        let header = fields(header);
        let columns = header
            .iter()
            .enumerate()
            .map(|(index, name)| (name.trim().to_owned(), index))
            .collect();
        let rows = lines.map(|line| (line.to_owned(), fields(line))).collect();
        Ok(Self {
            columns,
            width: header.len(),
            rows,
        })
    }

    /// The place of a column the adapter cannot do without. A file without
    /// it is not the file the adapter was written for.
    fn column(&self, name: &str, what: &str) -> Result<usize, SourceError> {
        self.columns
            .get(name)
            .copied()
            .ok_or_else(|| SourceError::Malformed(format!("{what} has no column {name:?}")))
    }

    /// The rows with as many fields as the header. Any other is reported.
    fn whole_rows<'a>(&'a self, parsed: &mut Parsed) -> Vec<(&'a str, &'a [String])> {
        let mut whole = Vec::with_capacity(self.rows.len());
        for (line, row) in &self.rows {
            if row.len() == self.width {
                whole.push((line.as_str(), row.as_slice()));
            } else {
                parsed.skipped.push(Skipped::new(
                    line,
                    format!(
                        "the row has {} fields and the header {}",
                        row.len(),
                        self.width
                    ),
                ));
            }
        }
        whole
    }
}

/// The text of a file: UTF-8 if it is valid UTF-8, else ISO 8859-1, whose
/// 256 characters are the first 256 of Unicode.
fn decode(body: &[u8]) -> String {
    match std::str::from_utf8(body) {
        Ok(text) => text.to_owned(),
        Err(_) => body.iter().copied().map(char::from).collect(),
    }
}

/// The fields of a line, split at semicolons. A field may be in double
/// quotes, with a semicolon inside and a quote written twice, as in
/// MeteoSwiss's description of its parameters.
fn fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ';' if !quoted => fields.push(std::mem::take(&mut field)),
            c => field.push(c),
        }
    }
    fields.push(field);
    fields
}

fn parse_stations(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    const WHAT: &str = "the station list";
    let table = Table::read(body, WHAT)?;
    let code = table.column("station_abbr", WHAT)?;
    let name = table.column("station_name", WHAT)?;
    let height = table.column("station_height_masl", WHAT)?;
    let lat = table.column("station_coordinates_wgs84_lat", WHAT)?;
    let lon = table.column("station_coordinates_wgs84_lon", WHAT)?;

    let mut parsed = Parsed::default();
    for (_, row) in table.whole_rows(&mut parsed) {
        let code = row[code].trim();
        let station = read_station(source, code, &row[name], &row[height], &row[lat], &row[lon]);
        match station {
            Ok(station) if ctx.area.contains(station.position) => parsed.stations.push(station),
            Ok(_) => {}
            Err(reason) => parsed.skipped.push(Skipped::new(code, reason)),
        }
    }
    Ok(parsed)
}

fn read_station(
    source: &SourceId,
    code: &str,
    name: &str,
    height: &str,
    lat: &str,
    lon: &str,
) -> Result<Station, String> {
    let degrees = |text: &str, what: &str| {
        text.trim()
            .parse::<f64>()
            .map_err(|_| format!("the {what} {text:?} cannot be read"))
    };
    let position = GeoPoint::new(degrees(lat, "latitude")?, degrees(lon, "longitude")?)
        .map_err(|e| e.to_string())?;
    // A station without a height has an empty field. A height that is there
    // and cannot be read leaves the station out: a wrong height is worse
    // than none, and none would hide the fault.
    let elevation_m = match height.trim() {
        "" => None,
        text => Some(
            text.parse::<f64>()
                .map_err(|_| format!("the height {text:?} cannot be read"))?,
        ),
    };
    let id = StationId::new(source.clone(), code).map_err(|e| e.to_string())?;
    Ok(Station {
        id,
        name: name.trim().to_owned(),
        position,
        elevation_m,
    })
}

fn parse_values(source: &SourceId, body: &[u8]) -> Result<Parsed, SourceError> {
    const WHAT: &str = "the values";
    let table = Table::read(body, WHAT)?;
    let code = table.column("Station/Location", WHAT)?;
    let date = table.column("Date", WHAT)?;
    let columns = PARAMETERS
        .iter()
        .map(|parameter| table.column(parameter.code, WHAT))
        .collect::<Result<Vec<_>, _>>()?;

    let mut parsed = Parsed::default();
    for (line, row) in table.whole_rows(&mut parsed) {
        let code = row[code].trim();
        let station = match StationId::new(source.clone(), code) {
            Ok(station) => station,
            Err(error) => {
                parsed
                    .skipped
                    .push(Skipped::new(line, format!("no station: {error}")));
                continue;
            }
        };
        let Some(time) = parse_time(&row[date]) else {
            parsed.skipped.push(Skipped::new(
                &format!("{code} {}", row[date]),
                "time stamp cannot be read",
            ));
            continue;
        };
        let mut observation = Observation::empty(station, time);
        for (parameter, &column) in PARAMETERS.iter().zip(&columns) {
            let text = row[column].trim();
            if text == "-" || text.is_empty() {
                continue;
            }
            match text.parse::<f64>() {
                Ok(value) if value.is_finite() => (parameter.set)(&mut observation, value),
                _ => parsed.skipped.push(Skipped::new(
                    &format!("{code} {} {text}", parameter.code),
                    "the value cannot be read as a number",
                )),
            }
        }
        // A row can hold nothing of the six: on 2026-11-22 the file had a
        // row of dashes only for MRP, a code the station list did not hold.
        if !observation.is_empty() {
            parsed.observations.push(observation);
        }
    }
    Ok(parsed)
}

/// Reads a time of the values file: UTC, to the minute, `202611220750`.
fn parse_time(text: &str) -> Option<Timestamp> {
    let text = text.trim();
    if text.len() != 12 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let time = NaiveDateTime::parse_from_str(text, "%Y%m%d%H%M").ok()?;
    Some(Utc.from_utc_datetime(&time))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_time_as_utc() {
        assert_eq!(
            parse_time("202611220750"),
            Some(Utc.with_ymd_and_hms(2026, 11, 22, 7, 50, 0).unwrap())
        );
        assert_eq!(
            parse_time("202612312350"),
            Some(Utc.with_ymd_and_hms(2026, 12, 31, 23, 50, 0).unwrap())
        );
        assert_eq!(parse_time("20261122075"), None);
        assert_eq!(parse_time("2026112207500"), None);
        assert_eq!(parse_time("18.11.2026 18:50"), None);
        assert_eq!(parse_time("202611222460"), None);
        assert_eq!(parse_time("+02611220750"), None);
    }

    #[test]
    fn kilometres_an_hour_become_metres_per_second() {
        assert_eq!(metres_per_second(0.0), 0.0);
        assert_eq!(metres_per_second(3.6), 1.0);
        assert_eq!(metres_per_second(36.0), 10.0);
        // Aigle's gust on 2026-11-22 at 07:50.
        assert_eq!(metres_per_second(53.6), 14.89);
        // A tenth of a km/h is still told apart.
        assert_eq!(metres_per_second(0.1), 0.03);
        assert_eq!(metres_per_second(31.3), 8.69);
    }

    #[test]
    fn splits_fields_with_and_without_quotes() {
        assert_eq!(
            fields("ABO;202611220750;-;0.60"),
            ["ABO", "202611220750", "-", "0.60"]
        );
        assert_eq!(
            fields(r#"fu3010z0;"Wind speed; ten minutes mean in km/h";km/h"#),
            ["fu3010z0", "Wind speed; ten minutes mean in km/h", "km/h"]
        );
        assert_eq!(fields(r#""a ""b"" c";d"#), [r#"a "b" c"#, "d"]);
        assert_eq!(fields(";"), ["", ""]);
    }

    #[test]
    fn reads_iso_8859_1_and_utf_8() {
        assert_eq!(decode(b"Ober\xe4geri"), "Oberägeri");
        assert_eq!(decode("Oberägeri".as_bytes()), "Oberägeri");
        assert_eq!(decode(b"Gen\xe8ve-Cointrin"), "Genève-Cointrin");
    }

    /// The units the adapter assumes are the ones MeteoSwiss's own
    /// description of its parameters gives, as recorded on 2026-11-22, and
    /// every parameter read is one of ten minutes (granularity `T`).
    #[test]
    fn the_units_are_meteoswiss_own() {
        let table = Table::read(
            include_bytes!("../tests/fixtures/ogd-smn_meta_parameters.csv"),
            "the description",
        )
        .unwrap();
        let code = table.column("parameter_shortname", "").unwrap();
        let unit = table.column("parameter_unit", "").unwrap();
        let granularity = table.column("parameter_granularity", "").unwrap();
        for parameter in &PARAMETERS {
            let row = table
                .rows
                .iter()
                .map(|(_, row)| row)
                .find(|row| row[code] == parameter.code)
                .unwrap_or_else(|| panic!("{} is not described", parameter.code));
            assert_eq!(row[unit], parameter.unit, "{}", parameter.code);
            assert_eq!(row[granularity], "T", "{}", parameter.code);
        }
    }

    /// swisstopo's approximate formulas from the Swiss coordinates to WGS84
    /// ("Approximate formulas for the transformation between Swiss
    /// projection coordinates and WGS84"), good to about a metre, for LV95.
    fn lv95_to_wgs84(east: f64, north: f64) -> (f64, f64) {
        let y = (east - 2_600_000.0) / 1_000_000.0;
        let x = (north - 1_200_000.0) / 1_000_000.0;
        let lon = 2.677_909_4 + 4.728_982 * y + 0.791_484 * y * x + 0.130_6 * y * x * x
            - 0.043_6 * y * y * y;
        let lat = 16.902_389_2 + 3.238_272 * x
            - 0.270_978 * y * y
            - 0.002_528 * x * x
            - 0.044_7 * y * y * x
            - 0.014_0 * x * x * x;
        (lat * 100.0 / 36.0, lon * 100.0 / 36.0)
    }

    #[test]
    fn the_formulas_give_the_example_of_their_document() {
        // 2,700,000 E and 1,100,000 N: 46° 2' 38.87" N, 8° 43' 49.79" E.
        let (lat, lon) = lv95_to_wgs84(2_700_000.0, 1_100_000.0);
        assert!(
            (lat - (46.0 + 2.0 / 60.0 + 38.87 / 3600.0)).abs() < 1e-5,
            "{lat}"
        );
        assert!(
            (lon - (8.0 + 43.0 / 60.0 + 49.79 / 3600.0)).abs() < 1e-5,
            "{lon}"
        );
    }

    /// The degrees of the station list are the stations' LV95 coordinates
    /// in WGS84, to a metre or two: neither swapped nor in another datum.
    #[test]
    fn the_degrees_of_the_list_agree_with_its_swiss_coordinates() {
        let table = Table::read(
            include_bytes!("../tests/fixtures/ogd-smn_meta_stations.csv"),
            "the list",
        )
        .unwrap();
        let column = |name| table.column(name, "").unwrap();
        let (east, north) = (
            column("station_coordinates_lv95_east"),
            column("station_coordinates_lv95_north"),
        );
        let (lat, lon) = (
            column("station_coordinates_wgs84_lat"),
            column("station_coordinates_wgs84_lon"),
        );
        assert_eq!(table.rows.len(), 158);
        for (_, row) in &table.rows {
            let number = |index: usize| row[index].parse::<f64>().unwrap();
            let (want_lat, want_lon) = lv95_to_wgs84(number(east), number(north));
            // 0.00002° is under two metres.
            assert!((number(lat) - want_lat).abs() < 2e-5, "{}", row[0]);
            assert!((number(lon) - want_lon).abs() < 2e-5, "{}", row[0]);
        }
        // Bern / Zollikofen, the station BER, at 2,601,934 E and
        // 1,204,410 N.
        let (_, ber) = table.rows.iter().find(|(_, row)| row[0] == "BER").unwrap();
        let (want_lat, want_lon) = lv95_to_wgs84(2_601_934.0, 1_204_410.0);
        assert_eq!(ber[lat], "46.990744");
        assert_eq!(ber[lon], "7.464061");
        assert!((want_lat - 46.990_744).abs() < 2e-5 && (want_lon - 7.464_061).abs() < 2e-5);
    }
}
