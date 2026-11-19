use std::io::{Cursor, Read};

use chrono::NaiveDateTime;
use venti_domain::{Observation, SourceId, StationId};
use venti_sources::{Parsed, Skipped, SourceError};

use crate::Dataset;

/// DWD writes a missing value as -999.
const MISSING: f64 = -999.0;

/// A column of a DWD table and the observation field it fills.
struct Column {
    name: &'static str,
    set: fn(&mut Observation, f64),
}

fn columns(dataset: Dataset) -> &'static [Column] {
    match dataset {
        Dataset::Wind => &[
            Column {
                name: "FF_10",
                set: |o, v| o.wind_speed_ms = Some(v),
            },
            Column {
                name: "DD_10",
                set: |o, v| o.wind_direction_deg = Some(v),
            },
        ],
        Dataset::Extremes => &[
            Column {
                name: "FX_10",
                set: |o, v| o.wind_gust_ms = Some(v),
            },
            Column {
                name: "DX_10",
                set: |o, v| o.gust_direction_deg = Some(v),
            },
        ],
        Dataset::Air => &[
            Column {
                name: "TT_10",
                set: |o, v| o.temperature_c = Some(v),
            },
            Column {
                name: "RF_10",
                set: |o, v| o.humidity_pct = Some(v),
            },
            Column {
                name: "PP_10",
                set: |o, v| o.pressure_hpa = Some(v),
            },
        ],
    }
}

/// Parses one station's zip file: a single semicolon-separated table whose
/// first line names the columns.
pub(crate) fn parse(
    source: &SourceId,
    dataset: Dataset,
    body: &[u8],
) -> Result<Parsed, SourceError> {
    let text = unzip_single_text_file(body)?;
    let mut lines = text.lines();

    let header: Vec<&str> = lines
        .next()
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .collect();
    let position = |name: &str| header.iter().position(|column| *column == name);
    let missing_column =
        |name: &str| SourceError::Malformed(format!("table has no column {name}: {header:?}"));
    let station_at = position("STATIONS_ID").ok_or_else(|| missing_column("STATIONS_ID"))?;
    let time_at = position("MESS_DATUM").ok_or_else(|| missing_column("MESS_DATUM"))?;
    let mut value_at = Vec::new();
    for column in columns(dataset) {
        let at = position(column.name).ok_or_else(|| missing_column(column.name))?;
        value_at.push((at, column));
    }

    let mut parsed = Parsed::default();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split(';').map(str::trim).collect();
        match parse_row(source, &cells, station_at, time_at, &value_at) {
            Ok(observation) => {
                if !observation.is_empty() {
                    parsed.observations.push(observation);
                }
            }
            Err(reason) => parsed.skipped.push(Skipped::new(line, reason)),
        }
    }
    Ok(parsed)
}

fn parse_row(
    source: &SourceId,
    cells: &[&str],
    station_at: usize,
    time_at: usize,
    value_at: &[(usize, &Column)],
) -> Result<Observation, String> {
    let cell = |at: usize| {
        cells
            .get(at)
            .copied()
            .ok_or_else(|| "row is cut short".to_owned())
    };

    // Rows carry the identifier without leading zeros; file names and the
    // station list use five digits.
    let raw_id = cell(station_at)?;
    if raw_id.is_empty() || raw_id.len() > 5 || !raw_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("station identifier {raw_id:?} is not a number"));
    }
    let station =
        StationId::new(source.clone(), format!("{raw_id:0>5}")).map_err(|e| e.to_string())?;

    let time = NaiveDateTime::parse_from_str(cell(time_at)?, "%Y%m%d%H%M")
        .map_err(|e| format!("time stamp: {e}"))?
        .and_utc();

    let mut observation = Observation::empty(station, time);
    for (at, column) in value_at {
        let text = cell(*at)?;
        let value: f64 = text.parse().map_err(|e| format!("{}: {e}", column.name))?;
        if value != MISSING {
            (column.set)(&mut observation, value);
        }
    }
    Ok(observation)
}

/// The largest table taken out of an archive: a megabyte. A station's file
/// holds the current day, at most 144 lines of some 60 characters, so this
/// is a hundred days' worth. A few kilobytes of archive can unpack to
/// gigabytes; what the archive says about its own content is not believed
/// either, and reading stops here.
const MAX_TABLE_BYTES: u64 = 1024 * 1024;

fn unzip_single_text_file(body: &[u8]) -> Result<String, SourceError> {
    let malformed = |what: String| SourceError::Malformed(what);
    let too_large = || {
        malformed(format!(
            "the file in the archive unpacks to more than {} kB",
            MAX_TABLE_BYTES / 1024
        ))
    };
    let mut archive = zip::ZipArchive::new(Cursor::new(body))
        .map_err(|e| malformed(format!("not a zip archive: {e}")))?;
    if archive.len() != 1 {
        return Err(malformed(format!(
            "expected one file in the archive, found {}",
            archive.len()
        )));
    }
    let file = archive
        .by_index(0)
        .map_err(|e| malformed(format!("cannot open the file in the archive: {e}")))?;
    if file.size() > MAX_TABLE_BYTES {
        return Err(too_large());
    }
    let mut text = String::new();
    file.take(MAX_TABLE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| malformed(format!("file in the archive is not text: {e}")))?;
    if text.len() as u64 > MAX_TABLE_BYTES {
        return Err(too_large());
    }
    Ok(text)
}
