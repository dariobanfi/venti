//! Foehn: the server's reading of the pressure across the Alps' main ridge,
//! in words for Today and for a view of its own.
//!
//! The shell asks the server ([`foehn_path`]) and hands the answer here
//! ([`foehn`]) with the places of the pilot's sites and the time. Out
//! comes one line for Today, which exists only when a pair of stations
//! near one of those places is at "possible" or "likely" on two pressures
//! that their providers reduced to sea level themselves, and a view:
//! each pair as a row with its words, its level and its last 24 hours as
//! points for a small chart, what the number means, the caution, the
//! credits, and every label a screen needs.
//!
//! The rule is the flight engine's ([`venti_flight::foehn`]), the same the
//! server answers with; it is asked again here, against the time the shell
//! names, because an answer may have waited on the device. A pair whose
//! reading is older than an hour has no level, and nothing here says "no
//! foehn": below the limits it is "no sign of foehn in the pressure".
//!
//! Against an unknown clock (`now_unix_s` zero or less) a reading has no
//! age: the pair is judged as its readings stand, and its words say the
//! reading's own time.

use chrono::{DateTime, Duration, Utc};
use venti_api_types::v1::{FOEHN_PATH, FoehnPair, FoehnReducedBy, FoehnResponse, FoehnStation};
use venti_domain::GeoPoint;
use venti_flight::foehn::{self, Judgement, Level, SeaLevelPressure, Side, Unknown};
use venti_flight::geodesy::Earth;

use crate::server::answer;
use crate::wind::credit;
use crate::{Coordinate, CoreError, Credit};

/// How near one of a pair's stations a place has to lie for the pair to
/// speak for it on Today, km. The foehn valleys of a pair reach about this
/// far from its stations. Ours, and to verify with pilots.
const NEAR_KM: f64 = 150.0;

/// What a pair's pressures say about foehn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum FoehnLevel {
    /// Nothing is known: a reading is missing or too old. Draw it greyed
    /// out; it is not "no foehn".
    Unknown,
    /// No sign of foehn in the pressure. Draw it plain, not as "good".
    None,
    /// Foehn is possible: the colour of a caution.
    Possible,
    /// Foehn is likely: the colour of a warning.
    Likely,
}

/// The one line for Today. A shell shows it only when it is there, in the
/// colour of its level, and opens the foehn view on a tap.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FoehnLine {
    /// "South foehn likely: 6 hPa higher at Bolzano than at Innsbruck".
    pub text: String,
    /// `Possible` or `Likely`, never less.
    pub level: FoehnLevel,
    /// The pair it speaks of ([`FoehnRow::id`]).
    pub pair_id: String,
}

/// One point of a pair's last 24 hours.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FoehnPoint {
    /// When, seconds since 1970-01-01 UTC.
    pub time_unix_s: i64,
    /// South less north, hPa: above zero the south is higher.
    pub difference_hpa: f64,
    /// The level a difference of this size has.
    pub level: FoehnLevel,
}

/// One pair of stations in the foehn view.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FoehnRow {
    /// A stable name for the pair.
    pub id: String,
    /// "Bolzano – Innsbruck": the south first.
    pub title: String,
    /// What the pair says now: "South foehn likely: 6 hPa higher at
    /// Bolzano than at Innsbruck", "No sign of foehn in the pressure: …",
    /// or why nothing can be said.
    pub words: String,
    /// The level, for the row's colour.
    pub level: FoehnLevel,
    /// The level in a word, for a badge: "Likely", "Possible", "No sign",
    /// "Not known".
    pub level_label: String,
    /// How old the reading is, "12 min ago", or against an unknown clock
    /// its own time, "18:50 UTC". Empty when the pair has no reading.
    pub when: String,
    /// The last 24 hours, oldest first, for a small chart with zero in the
    /// middle. Empty when the server holds none.
    pub points: Vec<FoehnPoint>,
    /// The chart's upper side: "Higher at Bolzano: south foehn".
    pub above_label: String,
    /// The chart's lower side: "Higher at Innsbruck: north foehn".
    pub below_label: String,
    /// Where the pair and its limits are from, and what of it waits to be
    /// verified: small print under the row. When Venti reduced a station's
    /// pressure to sea level itself, it says so here, that the number may
    /// be off by several hectopascals, and that the pair gives Today no
    /// line.
    pub note: String,
}

/// The foehn view, and the line for Today.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FoehnView {
    /// The view's title, "Foehn".
    pub title: String,
    /// The line for Today, when a pair near the pilot's sites is at
    /// "possible" or "likely" on pressures that both providers reduced to
    /// sea level themselves. Nothing otherwise: no line is not "no foehn",
    /// and a shell writes nothing in its place.
    pub today_line: Option<FoehnLine>,
    /// The pairs, in the server's order.
    pub rows: Vec<FoehnRow>,
    /// What the number means, in two plain sentences.
    pub explanation: String,
    /// What the reading does not say. Shown wherever a level is shown.
    pub caution: String,
    /// The heading over a row's chart, "Last 24 hours".
    pub chart_title: String,
    /// The chart's unit, "hPa".
    pub unit: String,
    /// The differences at which the chart draws its lines, with what each
    /// means: 3 and 6 hPa on either side of zero.
    pub limits: Vec<FoehnLimit>,
    /// What to say when the answer has no pair at all.
    pub empty: String,
    /// Whose data the pressures are.
    pub credits: Vec<Credit>,
}

/// A line across a pair's chart.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FoehnLimit {
    /// South less north, hPa.
    pub difference_hpa: f64,
    /// The level from this line outwards.
    pub level: FoehnLevel,
    /// "possible" or "likely".
    pub label: String,
}

/// The path to `GET` from the server for the foehn reading.
#[uniffi::export]
pub fn foehn_path() -> String {
    FOEHN_PATH.to_owned()
}

/// Reads the server's answer to [`foehn_path`]. `places` are where the
/// pilot's sites are, for the line on Today; without any there is no line.
/// `now_unix_s` is the current time by the server's clock; zero or less
/// means the clock is not known, and a reading's age becomes its own time.
#[uniffi::export]
pub fn foehn(
    status: u16,
    body: Vec<u8>,
    places: Vec<Coordinate>,
    now_unix_s: i64,
) -> Result<FoehnView, CoreError> {
    let response: FoehnResponse = answer(status, &body)?;
    let places: Vec<GeoPoint> = places
        .iter()
        .filter_map(|place| GeoPoint::new(place.lat, place.lon).ok())
        .collect();

    let mut today_line: Option<FoehnLine> = None;
    let mut rows = Vec::with_capacity(response.pairs.len());
    for pair in &response.pairs {
        let judgement = judge(pair, now_unix_s);
        // A pressure that Venti reduced itself, from the height the
        // provider lists for the station, was 3.7 hPa off at Innsbruck on
        // 2026-11-22: more than the step between two levels.
        let sure = matches!(judgement, Judgement::Known { sure: true, .. });
        let row = row(pair, judgement, now_unix_s);
        let near = places.iter().any(|place| {
            [&pair.south, &pair.north].iter().any(|station| {
                Earth::Wgs84.course(*place, station.position).distance_m <= NEAR_KM * 1000.0
            })
        });
        // The strongest level of the pairs near by; of two equal ones the
        // first. A pair whose stations or limits no weather service gave
        // (`to_verify`), and a pair with a pressure that Venti reduced to
        // sea level itself, is shown in the view and never speaks on
        // Today: a warning there has to rest on more than Venti's own
        // guess, at the limits and at the pressures.
        if near
            && !pair.to_verify
            && sure
            && matches!(row.level, FoehnLevel::Possible | FoehnLevel::Likely)
            && today_line
                .as_ref()
                .is_none_or(|line| rank(row.level) > rank(line.level))
        {
            today_line = Some(FoehnLine {
                text: row.words.clone(),
                level: row.level,
                pair_id: row.id.clone(),
            });
        }
        rows.push(row);
    }

    let limit = |sign: f64, hpa: f64, level: FoehnLevel, label: &str| FoehnLimit {
        difference_hpa: sign * hpa,
        level,
        label: label.to_owned(),
    };
    Ok(FoehnView {
        title: "Foehn".to_owned(),
        today_line,
        rows,
        explanation: format!(
            "The number is how much higher the air pressure, reduced to sea level, is on one \
             side of the Alps' main ridge than on the other: air is pushed over the ridge from \
             the higher side and falls into the valleys of the lower side as foehn. From {} hPa \
             foehn is possible and from {} hPa it is likely, and the larger the number the \
             stronger it tends to be.",
            whole(foehn::POSSIBLE_HPA),
            whole(foehn::LIKELY_HPA)
        ),
        caution: response.caution.clone(),
        chart_title: "Last 24 hours".to_owned(),
        unit: "hPa".to_owned(),
        limits: vec![
            limit(1.0, foehn::LIKELY_HPA, FoehnLevel::Likely, "likely"),
            limit(1.0, foehn::POSSIBLE_HPA, FoehnLevel::Possible, "possible"),
            limit(-1.0, foehn::POSSIBLE_HPA, FoehnLevel::Possible, "possible"),
            limit(-1.0, foehn::LIKELY_HPA, FoehnLevel::Likely, "likely"),
        ],
        empty: "This server reads no pressure across the Alps. That says nothing about foehn."
            .to_owned(),
        credits: response.attributions.iter().map(credit).collect(),
    })
}

fn rank(level: FoehnLevel) -> u8 {
    match level {
        FoehnLevel::Unknown => 0,
        FoehnLevel::None => 1,
        FoehnLevel::Possible => 2,
        FoehnLevel::Likely => 3,
    }
}

fn pressure(station: &FoehnStation) -> Option<SeaLevelPressure> {
    station.pressure.map(|pressure| SeaLevelPressure {
        time_unix_s: pressure.time.timestamp(),
        hpa: pressure.sea_level_hpa,
        reduced_here: pressure.reduced_by == FoehnReducedBy::Venti,
    })
}

/// The flight engine's rule over the pair's two newest pressures: against
/// the time the shell names, or, when that is not known, as they stand.
fn judge(pair: &FoehnPair, now_unix_s: i64) -> Judgement {
    let (south, north) = (pressure(&pair.south), pressure(&pair.north));
    if now_unix_s > 0 {
        return foehn::judge(south.as_ref(), north.as_ref(), now_unix_s);
    }
    match (south, north) {
        (None, _) => Judgement::Unknown(Unknown::Missing(Side::South)),
        (_, None) => Judgement::Unknown(Unknown::Missing(Side::North)),
        (Some(south), Some(north)) => foehn::judge_together(&south, &north),
    }
}

fn wire_level(level: Level) -> FoehnLevel {
    match level {
        Level::None => FoehnLevel::None,
        Level::Possible => FoehnLevel::Possible,
        Level::Likely => FoehnLevel::Likely,
    }
}

/// A whole number of hectopascals, as it is written.
fn whole(hpa: f64) -> String {
    format!("{hpa:.0}")
}

fn row(pair: &FoehnPair, judgement: Judgement, now_unix_s: i64) -> FoehnRow {
    let (south, north) = (&pair.south.name, &pair.north.name);
    let (level, words, when) = match judgement {
        Judgement::Known {
            difference,
            higher,
            level,
            sure: _,
        } => {
            let shown = whole(foehn::shown_hpa(difference.hpa));
            let measured = match higher {
                Some(Side::South) => format!("{shown} hPa higher at {south} than at {north}"),
                Some(Side::North) => format!("{shown} hPa higher at {north} than at {south}"),
                None => format!("the same pressure at {south} and at {north}"),
            };
            let kind = match higher {
                Some(Side::North) => "North foehn",
                _ => "South foehn",
            };
            let when = when(difference.time_unix_s, now_unix_s);
            // Against an unknown clock the words carry the reading's time:
            // nobody can say that it is the pressure now.
            let at = if now_unix_s > 0 {
                String::new()
            } else {
                format!(" at {when}")
            };
            let words = match level {
                Level::Likely => format!("{kind} likely{at}: {measured}"),
                Level::Possible => format!("{kind} possible{at}: {measured}"),
                Level::None => format!("No sign of foehn in the pressure{at}: {measured}"),
            };
            (wire_level(level), words, when)
        }
        Judgement::Unknown(why) => {
            let words = match why {
                Unknown::Missing(Side::South) => format!("Not known: no pressure from {south}"),
                Unknown::Missing(Side::North) => format!("Not known: no pressure from {north}"),
                Unknown::Old(Side::South) => {
                    format!("Not known: the pressure from {south} is older than an hour")
                }
                Unknown::Old(Side::North) => {
                    format!("Not known: the pressure from {north} is older than an hour")
                }
                Unknown::Apart => format!(
                    "Not known: the readings from {south} and {north} are too far apart in time"
                ),
            };
            // The older of the two readings, if there are two: the one
            // that keeps the pair from a level.
            let oldest = [&pair.south, &pair.north]
                .iter()
                .filter_map(|station| station.pressure)
                .map(|pressure| pressure.time.timestamp())
                .min();
            let when = oldest.map_or_else(String::new, |time| when(time, now_unix_s));
            (FoehnLevel::Unknown, words, when)
        }
    };
    let mut note = pair.basis.clone();
    let reduced_here: Vec<&str> = [&pair.south, &pair.north]
        .iter()
        .filter(|station| {
            station
                .pressure
                .is_some_and(|p| p.reduced_by == FoehnReducedBy::Venti)
        })
        .map(|station| station.name.as_str())
        .collect();
    if !reduced_here.is_empty() {
        note.push_str(&format!(
            " {} sent only the pressure at the station's own height, so Venti reduced it to \
             sea level itself, from the station's height and temperature. That can be off by \
             several hectopascals, so this number is less sure and gives Today no line.",
            reduced_here.join(" and ")
        ));
    }
    FoehnRow {
        id: pair.id.clone(),
        title: format!("{south} – {north}"),
        words,
        level,
        level_label: match level {
            FoehnLevel::Likely => "Likely",
            FoehnLevel::Possible => "Possible",
            FoehnLevel::None => "No sign",
            FoehnLevel::Unknown => "Not known",
        }
        .to_owned(),
        when,
        points: pair
            .series
            .iter()
            .map(|point| FoehnPoint {
                time_unix_s: point.time.timestamp(),
                difference_hpa: point.difference_hpa,
                level: wire_level(foehn::level(point.difference_hpa)),
            })
            .collect(),
        above_label: format!("Higher at {south}: south foehn"),
        below_label: format!("Higher at {north}: north foehn"),
        note,
    }
}

/// A reading's age against a known clock, and its own time against an
/// unknown one.
fn when(time_unix_s: i64, now_unix_s: i64) -> String {
    let time: DateTime<Utc> = DateTime::from_timestamp(time_unix_s, 0).unwrap_or_default();
    if now_unix_s <= 0 {
        return time.format("%H:%M UTC").to_string();
    }
    let now: DateTime<Utc> = DateTime::from_timestamp(now_unix_s, 0).unwrap_or_default();
    let age = (now - time).max(Duration::zero());
    if age < Duration::hours(1) {
        format!("{} min ago", age.num_minutes())
    } else {
        format!(
            "{} h {} min ago",
            age.num_hours(),
            age.num_minutes() - age.num_hours() * 60
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 18 November 2026, 19:05 UTC.
    const NOW: i64 = 1_795_028_700;

    /// An answer with one pair: Bolzano's and Innsbruck's pressures, each
    /// with its time, or none, both reduced by their providers.
    fn body(south: Option<(&str, f64)>, north: Option<(&str, f64)>) -> Vec<u8> {
        body_reduced_by(south, north, "provider")
    }

    /// The same with Innsbruck's pressure reduced by `north_by`.
    fn body_reduced_by(
        south: Option<(&str, f64)>,
        north: Option<(&str, f64)>,
        north_by: &str,
    ) -> Vec<u8> {
        let pressure = |reading: Option<(&str, f64)>, by: &str| {
            reading.map_or_else(String::new, |(time, hpa)| {
                format!(
                    r#", "pressure": {{"time": "{time}", "sea_level_hpa": {hpa}, "reduced_by": "{by}"}}"#
                )
            })
        };
        format!(
            r#"{{
            "time": "2026-11-18T19:05:00Z",
            "possible_from_hpa": 3.0, "likely_from_hpa": 6.0, "fresh_for_s": 3600,
            "pairs": [{{
                "id": "bolzano-innsbruck",
                "south": {{"id": "suedtirol-meteo:83200MS", "source": "suedtirol-meteo",
                           "name": "Bolzano", "position": {{"lat": 46.4977, "lon": 11.3128}}{}}},
                "north": {{"id": "geosphere-tawes:11320", "source": "geosphere-tawes",
                           "name": "Innsbruck", "position": {{"lat": 47.26, "lon": 11.3842}}{}}},
                "level": "none",
                "series": [
                    {{"time": "2026-11-18T16:50:00Z", "difference_hpa": 2.0}},
                    {{"time": "2026-11-18T18:50:00Z", "difference_hpa": 6.2}}
                ],
                "to_verify": false, "basis": "From the province's foehn chart.",
                "a_field_from_the_future": 1
            }}],
            "caution": "A sign, not a measurement.",
            "attributions": [
                {{"source": "geosphere-tawes", "provider": "GeoSphere Austria", "licence": "CC BY 4.0",
                  "text": "Data source: GeoSphere Austria", "url": "https://data.hub.geosphere.at"}}
            ]
        }}"#,
            pressure(south, "provider"),
            pressure(north, north_by)
        )
        .into_bytes()
    }

    /// The Wallberg above the Tegernsee, 56 km from Innsbruck; and Vienna.
    const WALLBERG: Coordinate = Coordinate {
        lat: 47.665,
        lon: 11.795,
    };
    const VIENNA: Coordinate = Coordinate {
        lat: 48.21,
        lon: 16.37,
    };

    fn view(south: Option<(&str, f64)>, north: Option<(&str, f64)>, now: i64) -> FoehnView {
        foehn(200, body(south, north), vec![WALLBERG], now).unwrap()
    }

    const AT_1850: &str = "2026-11-18T18:50:00Z";
    const AT_1900: &str = "2026-11-18T19:00:00Z";

    #[test]
    fn the_path_is_the_apis() {
        assert_eq!(foehn_path(), "/v1/foehn");
    }

    #[test]
    fn south_foehn_likely_is_a_line_on_today_and_a_row() {
        let view = view(Some((AT_1850, 1004.4)), Some((AT_1900, 998.2)), NOW);
        let line = view.today_line.unwrap();
        assert_eq!(
            line.text,
            "South foehn likely: 6 hPa higher at Bolzano than at Innsbruck"
        );
        assert_eq!(line.level, FoehnLevel::Likely);
        assert_eq!(line.pair_id, "bolzano-innsbruck");

        let row = &view.rows[0];
        assert_eq!(row.title, "Bolzano – Innsbruck");
        assert_eq!(row.words, line.text);
        assert_eq!(row.level_label, "Likely");
        assert_eq!(row.when, "15 min ago");
        assert_eq!(row.above_label, "Higher at Bolzano: south foehn");
        assert_eq!(row.below_label, "Higher at Innsbruck: north foehn");
        assert_eq!(row.points.len(), 2);
        assert_eq!(row.points[0].level, FoehnLevel::None);
        assert_eq!(row.points[1].level, FoehnLevel::Likely);
        // Both pressures are their providers' own: the note is the
        // pair's basis and no more.
        assert_eq!(row.note, "From the province's foehn chart.");

        assert_eq!(view.title, "Foehn");
        assert_eq!(view.caution, "A sign, not a measurement.");
        assert!(view.explanation.contains("From 3 hPa foehn is possible"));
        assert_eq!(view.explanation.matches(". ").count(), 1, "two sentences");
        assert_eq!(view.limits.len(), 4);
        assert_eq!(view.credits.len(), 1);
    }

    #[test]
    fn north_foehn_possible_names_the_northern_station_first() {
        let view = view(Some((AT_1850, 1011.0)), Some((AT_1900, 1015.0)), NOW);
        let line = view.today_line.unwrap();
        assert_eq!(
            line.text,
            "North foehn possible: 4 hPa higher at Innsbruck than at Bolzano"
        );
        assert_eq!(line.level, FoehnLevel::Possible);
    }

    #[test]
    fn a_pair_that_is_venti_s_own_guess_never_speaks_on_today() {
        let guessed = String::from_utf8(body(Some((AT_1850, 1021.2)), Some((AT_1900, 1015.0))))
            .unwrap()
            .replace(r#""to_verify": false"#, r#""to_verify": true"#)
            .into_bytes();
        let view = foehn(200, guessed, vec![WALLBERG], NOW).unwrap();
        assert_eq!(view.today_line, None);
        // The view still shows what the pressure says.
        assert_eq!(view.rows[0].level, FoehnLevel::Likely);
    }

    #[test]
    fn a_pressure_that_venti_reduced_gives_the_view_its_row_and_today_no_line() {
        // The morning of 22 November 2026 as Venti first said it: Bolzano
        // 1014.5 hPa by the province, Innsbruck 1007.8 hPa by Venti's own
        // reduction of the station's pressure.
        const AT_0900: &str = "2026-11-22T09:00:00Z";
        let now = 1_795_338_300;
        let ours = body_reduced_by(Some((AT_0900, 1014.5)), Some((AT_0900, 1007.8)), "venti");
        let view = foehn(200, ours, vec![WALLBERG], now).unwrap();
        assert_eq!(view.today_line, None);
        let row = &view.rows[0];
        assert_eq!(row.level, FoehnLevel::Likely);
        assert_eq!(
            row.words,
            "South foehn likely: 7 hPa higher at Bolzano than at Innsbruck"
        );
        assert!(
            row.note
                .starts_with("From the province's foehn chart. Innsbruck sent only")
        );
        assert!(row.note.contains("Venti reduced it to sea level itself"));
        assert!(row.note.contains("off by several hectopascals"));
        assert!(row.note.ends_with("gives Today no line."));
        // Against an unknown clock too.
        let ours = body_reduced_by(Some((AT_0900, 1014.5)), Some((AT_0900, 1007.8)), "venti");
        assert_eq!(
            foehn(200, ours, vec![WALLBERG], 0).unwrap().today_line,
            None
        );

        // The same morning with GeoSphere's own reduction of the same
        // reading, 1011.5 hPa: possible, and a line.
        let theirs = body(Some((AT_0900, 1014.5)), Some((AT_0900, 1011.5)));
        let view = foehn(200, theirs, vec![WALLBERG], now).unwrap();
        let line = view.today_line.unwrap();
        assert_eq!(
            line.text,
            "South foehn possible: 3 hPa higher at Bolzano than at Innsbruck"
        );
        assert_eq!(line.level, FoehnLevel::Possible);
        assert_eq!(view.rows[0].note, "From the province's foehn chart.");
    }

    #[test]
    fn below_the_limits_there_is_no_line_and_the_row_does_not_say_no_foehn() {
        let view = view(Some((AT_1850, 1016.0)), Some((AT_1900, 1015.0)), NOW);
        assert_eq!(view.today_line, None);
        let row = &view.rows[0];
        assert_eq!(row.level, FoehnLevel::None);
        assert_eq!(
            row.words,
            "No sign of foehn in the pressure: 1 hPa higher at Bolzano than at Innsbruck"
        );
        assert_eq!(row.level_label, "No sign");

        let same = self::view(Some((AT_1850, 1015.2)), Some((AT_1900, 1015.0)), NOW);
        assert_eq!(
            same.rows[0].words,
            "No sign of foehn in the pressure: the same pressure at Bolzano and at Innsbruck"
        );
    }

    #[test]
    fn a_stale_reading_gives_no_level_and_no_line_whatever_the_server_said() {
        // Nine hectopascals, and Bolzano's reading is from 17:00: the
        // answer waited on the device, or the station fell silent.
        let view = view(
            Some(("2026-11-18T17:00:00Z", 1024.0)),
            Some((AT_1900, 1015.0)),
            NOW,
        );
        assert_eq!(view.today_line, None);
        let row = &view.rows[0];
        assert_eq!(row.level, FoehnLevel::Unknown);
        assert_eq!(
            row.words,
            "Not known: the pressure from Bolzano is older than an hour"
        );
        assert_eq!(row.level_label, "Not known");
        assert_eq!(row.when, "2 h 5 min ago");

        // The same fresh answer, read two hours later.
        let later = self::view(
            Some((AT_1850, 1004.4)),
            Some((AT_1900, 998.2)),
            NOW + 2 * 3600,
        );
        assert_eq!(later.today_line, None);
        assert_eq!(later.rows[0].level, FoehnLevel::Unknown);
    }

    #[test]
    fn a_missing_reading_is_not_known_and_never_no_foehn() {
        for (south, north, words) in [
            (
                None,
                Some((AT_1900, 1015.0)),
                "Not known: no pressure from Bolzano",
            ),
            (
                Some((AT_1850, 1015.0)),
                None,
                "Not known: no pressure from Innsbruck",
            ),
            (None, None, "Not known: no pressure from Bolzano"),
        ] {
            let view = view(south, north, NOW);
            assert_eq!(view.today_line, None);
            assert_eq!(view.rows[0].level, FoehnLevel::Unknown);
            assert_eq!(view.rows[0].words, words);
            assert!(!view.rows[0].words.to_lowercase().contains("no foehn"));
            assert!(!view.rows[0].words.to_lowercase().contains("no sign"));
        }
    }

    #[test]
    fn against_an_unknown_clock_the_words_say_the_readings_own_time() {
        for now in [0, -1] {
            let view = view(Some((AT_1850, 1004.4)), Some((AT_1900, 998.2)), now);
            let row = &view.rows[0];
            assert_eq!(row.level, FoehnLevel::Likely);
            assert_eq!(
                row.words,
                "South foehn likely at 18:50 UTC: 6 hPa higher at Bolzano than at Innsbruck"
            );
            assert_eq!(row.when, "18:50 UTC");
            assert_eq!(view.today_line.as_ref().unwrap().text, row.words);
        }
    }

    #[test]
    fn the_line_is_only_for_a_pair_near_one_of_the_pilots_sites() {
        let likely = body(Some((AT_1850, 1004.4)), Some((AT_1900, 998.2)));
        let far = foehn(200, likely.clone(), vec![VIENNA], NOW).unwrap();
        assert_eq!(far.today_line, None);
        assert_eq!(far.rows[0].level, FoehnLevel::Likely);

        let none = foehn(200, likely.clone(), vec![], NOW).unwrap();
        assert_eq!(none.today_line, None);

        let one_near = foehn(200, likely, vec![VIENNA, WALLBERG], NOW).unwrap();
        assert!(one_near.today_line.is_some());
    }

    #[test]
    fn a_refusal_and_an_answer_without_pairs_are_said_as_what_they_are() {
        assert!(foehn(503, b"{}".to_vec(), vec![], NOW).is_err());
        let empty = br#"{"time": "2026-11-18T19:05:00Z", "possible_from_hpa": 3.0,
            "likely_from_hpa": 6.0, "fresh_for_s": 3600, "pairs": [], "caution": "c",
            "attributions": []}"#;
        let view = foehn(200, empty.to_vec(), vec![WALLBERG], NOW).unwrap();
        assert!(view.rows.is_empty());
        assert_eq!(view.today_line, None);
        assert!(view.empty.contains("says nothing about foehn"));
    }
}
