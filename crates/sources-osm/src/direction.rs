//! Reading the wind directions of a site.
//!
//! OpenStreetMap's `direction` key
//! (<https://wiki.openstreetmap.org/wiki/Key:direction>, read on 2026-11-19)
//! takes degrees from 0 to 360 clockwise from north, or one of the sixteen
//! points of the compass written with the letters N, E, S and W. Two of
//! them joined by a hyphen are a range, "calculated clockwise from the
//! given range border values", and several values are separated by
//! semicolons. The deprecated `free_flying:site_orientation` takes a list
//! of compass points and no ranges.
//!
//! What people write is looser than that, and this reader is strict where
//! being lenient would mean guessing:
//!
//! - A list may be separated by semicolons, commas or spaces. All three
//!   occur and none can be mistaken for anything else.
//! - A point written with an `O` is not read. It is east in German and
//!   west in Italian and French, and all three are spoken where the first
//!   region lies.
//! - A range that spans more than half the compass when read clockwise is
//!   not read. Of the ranges in the first region's data of 2026-11-18 a
//!   fifth do: `S-SE` is far more likely a mapper who wrote the ends the
//!   other way round than a take-off that works in every wind but one.
//!   `0-360`, which the key's page gives for "all around", is read.
//! - If any part of a value cannot be read, none of it is used. Half a
//!   list of directions would say that a site does not work in a wind it
//!   was tagged for.

use venti_domain::WindSector;

/// The sixteen points of the compass and the four words OpenStreetMap
/// allows for the main ones, with their degrees.
const COMPASS: [(&str, f64); 20] = [
    ("N", 0.0),
    ("NNE", 22.5),
    ("NE", 45.0),
    ("ENE", 67.5),
    ("E", 90.0),
    ("ESE", 112.5),
    ("SE", 135.0),
    ("SSE", 157.5),
    ("S", 180.0),
    ("SSW", 202.5),
    ("SW", 225.0),
    ("WSW", 247.5),
    ("W", 270.0),
    ("WNW", 292.5),
    ("NW", 315.0),
    ("NNW", 337.5),
    ("NORTH", 0.0),
    ("EAST", 90.0),
    ("SOUTH", 180.0),
    ("WEST", 270.0),
];

/// One direction: a compass point or a number of degrees.
fn direction(text: &str) -> Result<f64, String> {
    let upper = text.to_ascii_uppercase();
    if let Some((_, degrees)) = COMPASS.iter().find(|(name, _)| *name == upper) {
        return Ok(*degrees);
    }
    let looks_like_a_number =
        !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    if looks_like_a_number {
        return match text.parse::<f64>() {
            Ok(degrees) if (0.0..=360.0).contains(&degrees) => Ok(degrees),
            _ => Err(format!("{text:?} is not a direction from 0 to 360 degrees")),
        };
    }
    if upper.contains('O') {
        return Err(format!(
            "{text:?} is written with an O, which is east in German and west in Italian and French"
        ));
    }
    Err(format!("{text:?} is not a point of the compass"))
}

/// Reads the value of a direction tag into spans of wind direction, in the
/// order they are written, each once.
///
/// `ranges` says whether the key the value comes from defines ranges, as
/// `direction` does and `free_flying:site_orientation` does not. `Err`
/// says why the value cannot be read for certain; see the module's
/// description for what that covers.
pub fn directions(value: &str, ranges: bool) -> Result<Vec<WindSector>, String> {
    let mut sectors: Vec<WindSector> = Vec::new();
    let parts = value
        .split(|c: char| c == ';' || c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty());
    for part in parts {
        let sector = match part.split_once('-') {
            None => {
                // 360 is north again.
                WindSector::single(direction(part)? % 360.0)
            }
            Some(_) if !ranges => {
                return Err(format!(
                    "{part:?} is a range, which this key does not define"
                ));
            }
            Some((from, to)) => {
                let (from_deg, to_deg) = (direction(from)?, direction(to)?);
                if from_deg == 0.0 && to_deg == 360.0 {
                    WindSector { from_deg, to_deg }
                } else {
                    let (from_deg, to_deg) = (from_deg % 360.0, to_deg % 360.0);
                    let span = (to_deg - from_deg).rem_euclid(360.0);
                    if span > 180.0 {
                        return Err(format!(
                            "{part:?} spans {span} degrees when read clockwise; \
                             its ends may be written the other way round"
                        ));
                    }
                    WindSector { from_deg, to_deg }
                }
            }
        };
        if !sectors.contains(&sector) {
            sectors.push(sector);
        }
    }
    if sectors.is_empty() {
        return Err("it names no direction".to_owned());
    }
    Ok(sectors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(value: &str, ranges: bool) -> Vec<(f64, f64)> {
        directions(value, ranges)
            .unwrap_or_else(|reason| panic!("{value:?}: {reason}"))
            .into_iter()
            .map(|sector| (sector.from_deg, sector.to_deg))
            .collect()
    }

    #[test]
    fn single_points_and_degrees_are_read() {
        assert_eq!(spans("S", true), [(180.0, 180.0)]);
        assert_eq!(spans("WNW", true), [(292.5, 292.5)]);
        assert_eq!(spans("135", true), [(135.0, 135.0)]);
        assert_eq!(spans("22.5", true), [(22.5, 22.5)]);
        assert_eq!(spans("360", true), [(0.0, 0.0)]);
        assert_eq!(spans("nw", false), [(315.0, 315.0)]);
        assert_eq!(spans("north", true), [(0.0, 0.0)]);
    }

    #[test]
    fn lists_are_read_whatever_separates_them() {
        // As written in the first region's data.
        let south = [(225.0, 225.0), (180.0, 180.0), (135.0, 135.0)];
        assert_eq!(spans("SW;S;SE", true), south);
        assert_eq!(spans("SW S SE", false), south);
        assert_eq!(spans("SW, S ,SE", false), south);
        assert_eq!(
            spans("WNW,W,E", false),
            [(292.5, 292.5), (270.0, 270.0), (90.0, 90.0)]
        );
        // Each direction once.
        assert_eq!(spans("S;S;SW", true), [(180.0, 180.0), (225.0, 225.0)]);
    }

    #[test]
    fn ranges_run_clockwise_as_the_key_defines_them() {
        assert_eq!(spans("SE-SW", true), [(135.0, 225.0)]);
        assert_eq!(spans("E-WSW", true), [(90.0, 247.5)]);
        assert_eq!(
            spans("22-112;SSW-WNW", true),
            [(22.0, 112.0), (202.5, 292.5)]
        );
        // Across north.
        assert_eq!(spans("NW-NE", true), [(315.0, 45.0)]);
        // Half the compass is the widest span that is read.
        assert_eq!(spans("E-W", true), [(90.0, 270.0)]);
        // All around, as the key's page writes it.
        assert_eq!(spans("0-360", true), [(0.0, 360.0)]);
        assert_eq!(spans("S-S", true), [(180.0, 180.0)]);
    }

    #[test]
    fn what_cannot_be_read_for_certain_is_not_read() {
        let refused = |value: &str, ranges: bool, why: &str| {
            let reason = directions(value, ranges).unwrap_err();
            assert!(reason.contains(why), "{value:?}: {reason}");
        };
        // East in German, west in Italian.
        refused("O", false, "written with an O");
        refused("N,NO,O", false, "written with an O");
        refused("NNO-O", true, "written with an O");
        refused("SO", false, "written with an O");
        // Read clockwise these span most of the compass.
        refused("S-SE", true, "other way round");
        refused("SE-NE", true, "270 degrees");
        // The deprecated key has no ranges.
        refused("SE-SW", false, "does not define");
        // One part that cannot be read spoils the value.
        refused("S;SW;X", true, "not a point of the compass");
        refused("S-SWW", true, "not a point of the compass");
        refused("X", false, "not a point of the compass");
        refused("400", true, "0 to 360");
        refused("-90", true, "not a point of the compass");
        refused("S - SW", true, "not a point of the compass");
        refused("", true, "names no direction");
        refused(" ; ", true, "names no direction");
    }
}
