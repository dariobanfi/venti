use chrono::{Duration, NaiveDate};
use venti_domain::{GeoPoint, SourceId, Station, StationId};
use venti_sources::{Parsed, PlanCtx, Skipped, SourceError};

use crate::Dataset;

/// A station whose last listed day of data is older than this is treated as
/// no longer reporting, and its files are not requested.
const STALE_AFTER: Duration = Duration::days(2);

/// Parses a DWD station list and asks for the data file of every station that
/// is inside the area and still reporting.
///
/// The list is fixed-width text: identifier, first and last day of data,
/// height, latitude, longitude, then name, state and a release flag separated
/// by runs of spaces.
pub(crate) fn parse(
    source: &SourceId,
    dataset: Dataset,
    body: &[u8],
    ctx: &PlanCtx,
) -> Result<Parsed, SourceError> {
    // Latin-1 maps every byte to the Unicode code point of the same number.
    let text: String = body.iter().map(|&byte| char::from(byte)).collect();
    let mut lines = text.lines();

    let header = lines.next().unwrap_or_default();
    if !header.starts_with("Stations_id") {
        return Err(SourceError::Malformed(format!(
            "station list does not start with the expected header: {:?}",
            header.chars().take(60).collect::<String>()
        )));
    }

    let oldest_current_day = ctx.now.date_naive() - STALE_AFTER;
    let mut parsed = Parsed::default();
    for line in lines {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('-') {
            continue;
        }
        match parse_line(line) {
            Ok(entry) => {
                let inside = ctx.area.contains(entry.position);
                let current = entry.last_day >= oldest_current_day;
                if !(inside && current) {
                    continue;
                }
                match StationId::new(source.clone(), entry.id.clone()) {
                    Ok(id) => {
                        parsed.follow_up.push(dataset.data_request(&entry.id));
                        parsed.stations.push(Station {
                            id,
                            name: entry.name,
                            position: entry.position,
                            elevation_m: Some(entry.height_m),
                        });
                    }
                    Err(error) => parsed.skipped.push(Skipped::new(line, error.to_string())),
                }
            }
            Err(reason) => parsed.skipped.push(Skipped::new(line, reason)),
        }
    }
    Ok(parsed)
}

struct Entry {
    id: String,
    last_day: NaiveDate,
    height_m: f64,
    position: GeoPoint,
    name: String,
}

fn parse_line(line: &str) -> Result<Entry, String> {
    let mut rest = line;
    let mut next = |what: &str| -> Result<&str, String> {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (token, tail) = rest.split_at(end);
        rest = tail;
        if token.is_empty() {
            Err(format!("{what} is missing"))
        } else {
            Ok(token)
        }
    };

    let id = next("station identifier")?.to_owned();
    if id.len() != 5 || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("station identifier is not five digits".to_owned());
    }
    let _first_day = next("first day")?;
    let last_day = NaiveDate::parse_from_str(next("last day")?, "%Y%m%d")
        .map_err(|e| format!("last day: {e}"))?;
    let height_m: f64 = next("height")?
        .parse()
        .map_err(|e| format!("height: {e}"))?;
    let lat: f64 = next("latitude")?
        .parse()
        .map_err(|e| format!("latitude: {e}"))?;
    let lon: f64 = next("longitude")?
        .parse()
        .map_err(|e| format!("longitude: {e}"))?;
    let position = GeoPoint::new(lat, lon).map_err(|e| e.to_string())?;

    // The name may contain single spaces; the next column starts after a run of them.
    let name = rest
        .trim()
        .split("  ")
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if name.is_empty() {
        return Err("station name is missing".to_owned());
    }
    Ok(Entry {
        id,
        last_day,
        height_m,
        position,
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_line_with_spaces_and_non_ascii_letters_in_the_name() {
        let line = "02290 19931209 20261118            977     47.8009   11.0108 Hohenpeißenberg                          Bayern                                   Frei";
        let entry = parse_line(line).unwrap();
        assert_eq!(entry.id, "02290");
        assert_eq!(entry.name, "Hohenpeißenberg");
        assert_eq!(entry.height_m, 977.0);
        assert_eq!(
            entry.last_day,
            NaiveDate::from_ymd_opt(2026, 11, 18).unwrap()
        );

        let line = "00011 19920917 20261118            680     47.9736    8.5205 Donaueschingen (Landeplatz)              Baden-Württemberg                        Frei";
        assert_eq!(
            parse_line(line).unwrap().name,
            "Donaueschingen (Landeplatz)"
        );
    }

    #[test]
    fn explains_what_is_wrong_with_a_bad_line() {
        assert!(parse_line("5792 19940731 20261118 2956 47.4210 10.9848 Zugspitze").is_err());
        assert!(parse_line("05792 19940731 2026 2956 47.4210 10.9848 Zugspitze").is_err());
        assert!(parse_line("05792 19940731 20261118 2956 147.4210 10.9848 Zugspitze").is_err());
        assert!(parse_line("05792 19940731 20261118").is_err());
    }
}
