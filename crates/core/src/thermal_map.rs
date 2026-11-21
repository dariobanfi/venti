//! The thermal map: the kind of day, square by square, over the area the
//! server has forecasts for.
//!
//! The server judges one cell of a weather model for every square of a
//! grid, with the flight engine's rules ([`venti_flight::day`]), and hands
//! out the result as one answer. The shell asks for it
//! ([`thermal_map_path`]) and hands the answer here; out come the squares
//! of each day as a map draws them, with the words for the square a pilot
//! taps.
//!
//! A square is judged at the height of the model's ground, which in the
//! mountains lies below most take-offs: the thermals have further to go
//! from there, so a square can look better than a take-off in it. A
//! take-off's own page ([`crate::site_outlook`]) judges at the take-off's
//! height and knows the directions it works in. The map is where to look;
//! the page is what to read before going.

use chrono::{DateTime, Duration, NaiveDate};
use venti_api_types::v1;

use crate::forecast::model_name;
use crate::server::answer;
use crate::wind::credit;
use crate::{CoreError, Credit, Flyability, Thermals};

/// The most cells a map may have and still be read. The first region has
/// some 1,800; a hundred times that is a server that is not ours.
const MAX_CELLS: usize = 200_000;

/// The steps of the design language's thermal scale the kinds of day are
/// drawn in, from weak thermals to a big day. The scale has six steps and
/// is made for a climb in m/s; the kinds take four of them, spaced so that
/// neighbours on a map can be told apart. Colours of their own for the
/// kinds of day are the owner's to give.
const SCALE_STEPS: [u8; 4] = [1, 2, 4, 6];

/// One square of the thermal map on one day.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapCell {
    /// The square's southern edge, degrees of latitude.
    pub south: f64,
    /// Its western edge, degrees of longitude.
    pub west: f64,
    /// Its northern edge.
    pub north: f64,
    /// Its eastern edge.
    pub east: f64,
    /// Whether the day can be flown there, by the air alone.
    pub verdict: Flyability,
    /// The kind of day its thermals make. None on a day that is not
    /// flyable, and where the model gives no estimate.
    pub thermals: Option<Thermals>,
    /// The step of the thermal scale to draw the square in, 1 to 6. None
    /// where there is no kind of day: draw a day that is not flyable in
    /// the status colour for it, and nothing where there is no estimate.
    pub scale_step: Option<u8>,
    /// The verdict in words: "Flyable", "Marginal", "Not flyable".
    pub status: String,
    /// The kind of day in words: "Cross-country day". Empty without one.
    pub thermal_words: String,
    /// The thermals in a line: "1.8 m/s to a cloud base of 3250 m",
    /// "1.4 m/s to 2900 m in the blue", "0.3 m/s to 1800 m at best",
    /// "None to climb in". Empty without a kind of day.
    pub thermal_line: String,
    /// The height the square was judged at: the model's ground there,
    /// metres above mean sea level. None where the server does not say.
    pub ground_m: Option<f64>,
}

/// One day of the thermal map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapDay {
    /// The day as a date: "2026-07-12". It is the day by the sun at the
    /// squares.
    pub date: String,
    /// What to call it: "Today", "Tomorrow", else the day of the week,
    /// "Mon".
    pub name: String,
    /// Whether every square was judged with all of the day's daylight
    /// hours.
    pub whole: bool,
    /// What to say under the map on a day that is not whole. Empty on one
    /// that is.
    pub note: String,
    /// The squares that have a verdict, from the south-west, row by row.
    pub cells: Vec<ThermalMapCell>,
}

/// One line of the map's key.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalLegendEntry {
    /// What the colour stands for: "Big day".
    pub words: String,
    /// The step of the thermal scale it is drawn in. None for the line of
    /// a day that is not flyable, which is drawn in the status colour.
    pub scale_step: Option<u8>,
}

/// The thermal map as a shell draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMap {
    /// Today and the days after it that the map has, the earliest first.
    pub days: Vec<ThermalMapDay>,
    /// The key: the kinds of day from the best down, then the day that is
    /// not flyable.
    pub legend: Vec<ThermalLegendEntry>,
    /// The weather model, by the name pilots know: "ICON-D2".
    pub model: String,
    /// When the newest run began that the map holds hours of, seconds
    /// since 1970.
    pub run_unix_s: i64,
    /// When the server worked the map out.
    pub computed_unix_s: i64,
    /// What the map is and is not, for the foot of the screen.
    pub note: String,
    /// The credits to show wherever the map is.
    pub credits: Vec<Credit>,
}

/// The path to `GET` from the server for the thermal map.
#[uniffi::export]
pub fn thermal_map_path() -> String {
    v1::THERMALS_PATH.to_owned()
}

/// Reads the server's answer to [`thermal_map_path`]. `utc_offset_s` is
/// how far the pilot's clock is ahead of UTC and `now_unix_s` the moment
/// by Venti's clock: they decide which day is called today, and days
/// before it are left out.
#[uniffi::export]
pub fn thermal_map(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
    now_unix_s: i64,
) -> Result<ThermalMap, CoreError> {
    let response: v1::ThermalMapResponse = answer(status, &body)?;
    let grid = response.grid;
    let cells = grid.cells();
    let unreadable = || CoreError::Server {
        message:
            "The server's thermal map could not be read. The app may be older than the server."
                .to_owned(),
    };
    // Every list is as long as the grid has cells, or the map is not one.
    let fits = |day: &v1::ThermalMapDay| {
        [
            day.verdict.len(),
            day.thermals.len(),
            day.climb_ms.len(),
            day.top_m.len(),
            day.cloud.len(),
        ]
        .iter()
        .all(|length| *length == cells)
    };
    if cells > MAX_CELLS
        || !grid.step_deg.is_finite()
        || grid.step_deg <= 0.0
        || response.ground_m.len() != cells
        || !response.days.iter().all(fits)
    {
        return Err(unreadable());
    }

    let today = (DateTime::from_timestamp(now_unix_s, 0).unwrap_or_default()
        + Duration::seconds(i64::from(utc_offset_s)))
    .date_naive();
    let name = model_name(response.model.as_str());
    let days = response
        .days
        .iter()
        .filter(|day| day.date >= today)
        .map(|day| ThermalMapDay {
            date: day.date.format("%Y-%m-%d").to_string(),
            name: day_name(day.date, today),
            whole: day.whole,
            note: if day.whole {
                String::new()
            } else {
                "Judged by the hours the model's runs at hand have of this day, which are not \
                 all of its daylight: it can look worse here than it is."
                    .to_owned()
            },
            cells: (0..cells)
                .filter_map(|cell| square(&grid, day, &response.ground_m, cell))
                .collect(),
        })
        .collect();
    Ok(ThermalMap {
        days,
        legend: legend(),
        note: format!(
            "Read from {name}. Each square stands for one cell of the model in it, judged at \
             the height of the model's ground by the wind, the rain and the heat the ground \
             gives the air. An estimate by fixed limits, and no advice to fly: it knows nothing \
             of a slope or a take-off. A take-off's own page judges at the take-off's height."
        ),
        model: name,
        run_unix_s: response.run.timestamp(),
        computed_unix_s: response.computed_at.timestamp(),
        credits: response.attributions.iter().map(credit).collect(),
    })
}

/// What to call a day that is `date`, when today is `today`.
fn day_name(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "Today".to_owned(),
        1 => "Tomorrow".to_owned(),
        _ => date.format("%a").to_string(),
    }
}

/// The key of the map.
fn legend() -> Vec<ThermalLegendEntry> {
    [
        Thermals::Big,
        Thermals::CrossCountry,
        Thermals::Local,
        Thermals::Weak,
    ]
    .into_iter()
    .map(|kind| ThermalLegendEntry {
        words: kind.words().to_owned(),
        scale_step: Some(scale_step(kind)),
    })
    .chain([ThermalLegendEntry {
        words: Flyability::NotFlyable.words().to_owned(),
        scale_step: None,
    }])
    .collect()
}

/// The step of the thermal scale a kind of day is drawn in.
fn scale_step(kind: Thermals) -> u8 {
    match kind {
        Thermals::Weak => SCALE_STEPS[0],
        Thermals::Local => SCALE_STEPS[1],
        Thermals::CrossCountry => SCALE_STEPS[2],
        Thermals::Big => SCALE_STEPS[3],
    }
}

/// One square of a day, if the day has a verdict for it and the square
/// lies on the globe.
fn square(
    grid: &v1::ThermalGrid,
    day: &v1::ThermalMapDay,
    ground_m: &[Option<i32>],
    cell: usize,
) -> Option<ThermalMapCell> {
    let verdict = match day.verdict.get(cell).copied()?? {
        v1::ThermalVerdict::Flyable => Flyability::Flyable,
        v1::ThermalVerdict::Marginal => Flyability::Marginal,
        v1::ThermalVerdict::NotFlyable => Flyability::NotFlyable,
    };
    let middle = grid.point(cell)?;
    let half = grid.step_deg / 2.0;
    // A day that is not flyable says nothing of thermals, whatever the
    // answer holds.
    let thermals = day
        .thermals
        .get(cell)
        .copied()
        .flatten()
        .filter(|_| verdict != Flyability::NotFlyable)
        .map(|kind| match kind {
            v1::ThermalKind::Weak => Thermals::Weak,
            v1::ThermalKind::Local => Thermals::Local,
            v1::ThermalKind::CrossCountry => Thermals::CrossCountry,
            v1::ThermalKind::Big => Thermals::Big,
        });
    let climb_ms = day.climb_ms.get(cell).copied().flatten();
    let top_m = day.top_m.get(cell).copied().flatten();
    let cloud = day.cloud.get(cell).copied().unwrap_or(false);
    Some(ThermalMapCell {
        south: (middle.lat() - half).max(-90.0),
        west: middle.lon() - half,
        north: (middle.lat() + half).min(90.0),
        east: middle.lon() + half,
        verdict,
        thermals,
        scale_step: thermals.map(scale_step),
        status: verdict.words().to_owned(),
        thermal_words: thermals.map_or_else(String::new, |kind| kind.words().to_owned()),
        thermal_line: thermals.map_or_else(String::new, |kind| line(kind, climb_ms, top_m, cloud)),
        ground_m: ground_m.get(cell).copied().flatten().map(f64::from),
    })
}

/// A square's thermals in a line.
fn line(kind: Thermals, climb_ms: Option<f64>, top_m: Option<i32>, cloud: bool) -> String {
    match (kind, climb_ms, top_m) {
        (Thermals::Weak, Some(climb), Some(top)) if climb > 0.0 => {
            format!("{climb:.1} m/s to {top} m at best")
        }
        (Thermals::Weak, _, _) => "None to climb in".to_owned(),
        (_, Some(climb), Some(top)) if cloud => {
            format!("{climb:.1} m/s to a cloud base of {top} m")
        }
        (_, Some(climb), Some(top)) => format!("{climb:.1} m/s to {top} m in the blue"),
        // A kind of day without its figures: say the kind and no more.
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map of three squares in a row for two days, as a server sends it.
    fn body() -> Vec<u8> {
        br#"{
            "source": "open-meteo", "model": "icon-d2",
            "run": "2026-07-12T03:00:00Z", "computed_at": "2026-07-12T04:41:00Z",
            "grid": {"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": 1, "columns": 3},
            "ground_m": [612, 1240, null],
            "days": [
                {"date": "2026-07-11", "whole": true,
                 "verdict": ["flyable", "flyable", "flyable"],
                 "thermals": ["local", "local", "local"],
                 "climb_ms": [0.7, 0.7, 0.7], "top_m": [1800, 1800, 1800],
                 "cloud": [false, false, false]},
                {"date": "2026-07-12", "whole": false,
                 "verdict": ["flyable", "not_flyable", null],
                 "thermals": ["cross_country", "big", null],
                 "climb_ms": [1.8, 2.4, null], "top_m": [3250, 3400, null],
                 "cloud": [true, false, false]},
                {"date": "2026-07-13", "whole": true,
                 "verdict": ["marginal", "flyable", "flyable"],
                 "thermals": ["weak", "weak", null],
                 "climb_ms": [0.3, 0.0, null], "top_m": [1800, null, null],
                 "cloud": [false, false, false]},
                {"date": "2026-07-14", "whole": true,
                 "verdict": ["flyable", "flyable", "flyable"],
                 "thermals": ["big", "local", "big"],
                 "climb_ms": [2.2, 0.6, 2.6], "top_m": [3400, 2100, 3600],
                 "cloud": [true, true, false]}
            ],
            "attributions": [{"source": "open-meteo", "provider": "Open-Meteo",
                "licence": "CC BY 4.0", "text": "Weather data by Open-Meteo.com",
                "url": "https://open-meteo.com/"}]
        }"#
        .to_vec()
    }

    /// Noon on 12 July 2026 by UTC.
    const NOON: i64 = 1_783_857_600;

    fn map() -> ThermalMap {
        thermal_map(200, body(), 7200, NOON).unwrap()
    }

    #[test]
    fn the_path_is_the_servers() {
        assert_eq!(thermal_map_path(), "/v1/thermals");
    }

    #[test]
    fn days_are_named_by_the_pilots_clock_and_the_past_is_left_out() {
        let map = map();
        let names: Vec<&str> = map.days.iter().map(|day| day.name.as_str()).collect();
        assert_eq!(names, ["Today", "Tomorrow", "Tue"]);
        assert_eq!(map.days[0].date, "2026-07-12");

        // An hour before midnight in Greenwich it is the next day already
        // two hours east of it.
        let late = thermal_map(200, body(), 7200, NOON + 11 * 3600).unwrap();
        assert_eq!(late.days[0].date, "2026-07-13");
        assert_eq!(late.days[0].name, "Today");
    }

    #[test]
    fn a_square_lies_half_a_step_around_its_point() {
        let map = map();
        let cell = &map.days[0].cells[0];
        assert!((cell.south - 46.95).abs() < 1e-9 && (cell.north - 47.05).abs() < 1e-9);
        assert!((cell.west - 10.95).abs() < 1e-9 && (cell.east - 11.05).abs() < 1e-9);
        assert_eq!(cell.ground_m, Some(612.0));
    }

    #[test]
    fn a_square_says_its_kind_of_day_in_words_and_in_a_step_of_the_scale() {
        let map = map();
        let today = &map.days[0].cells;
        // The third square has no verdict today: it is not drawn.
        assert_eq!(today.len(), 2);
        assert_eq!(today[0].status, "Flyable");
        assert_eq!(today[0].thermals, Some(Thermals::CrossCountry));
        assert_eq!(today[0].thermal_words, "Cross-country day");
        assert_eq!(today[0].thermal_line, "1.8 m/s to a cloud base of 3250 m");
        assert_eq!(today[0].scale_step, Some(4));

        let later = &map.days[2].cells;
        assert_eq!(later[0].thermal_line, "2.2 m/s to a cloud base of 3400 m");
        assert_eq!(later[0].scale_step, Some(6));
        assert_eq!(later[1].thermal_words, "Local thermals");
        assert_eq!(later[1].scale_step, Some(2));
        assert_eq!(later[2].thermal_line, "2.6 m/s to 3600 m in the blue");
    }

    #[test]
    fn a_day_that_is_not_flyable_names_no_thermals_whatever_the_answer_holds() {
        let map = map();
        let cell = &map.days[0].cells[1];
        assert_eq!(cell.verdict, Flyability::NotFlyable);
        assert_eq!(cell.status, "Not flyable");
        assert_eq!(cell.thermals, None);
        assert_eq!(cell.scale_step, None);
        assert_eq!(cell.thermal_words, "");
        assert_eq!(cell.thermal_line, "");
        assert_eq!(cell.ground_m, Some(1240.0));
    }

    #[test]
    fn weak_thermals_say_the_best_there_is_or_that_there_is_none() {
        let map = map();
        let tomorrow = &map.days[1].cells;
        assert_eq!(tomorrow[0].status, "Marginal");
        assert_eq!(tomorrow[0].thermal_words, "Weak thermals");
        assert_eq!(tomorrow[0].thermal_line, "0.3 m/s to 1800 m at best");
        assert_eq!(tomorrow[0].scale_step, Some(1));
        assert_eq!(tomorrow[1].thermal_line, "None to climb in");
        // Flyable, and the model gave no estimate: a verdict and no more.
        assert_eq!(tomorrow[2].status, "Flyable");
        assert_eq!(tomorrow[2].thermals, None);
        assert_eq!(tomorrow[2].scale_step, None);
    }

    #[test]
    fn a_day_that_is_not_whole_says_so() {
        let map = map();
        assert!(!map.days[0].whole);
        assert!(map.days[0].note.contains("not all of its daylight"));
        assert!(map.days[1].whole);
        assert_eq!(map.days[1].note, "");
    }

    #[test]
    fn the_map_names_its_model_its_run_and_its_key() {
        let map = map();
        assert_eq!(map.model, "ICON-D2");
        assert_eq!(map.run_unix_s, 1_783_825_200);
        assert!(map.note.starts_with("Read from ICON-D2."));
        assert!(map.note.contains("no advice to fly"));
        assert_eq!(map.credits.len(), 1);
        let key: Vec<(&str, Option<u8>)> = map
            .legend
            .iter()
            .map(|entry| (entry.words.as_str(), entry.scale_step))
            .collect();
        assert_eq!(
            key,
            [
                ("Big day", Some(6)),
                ("Cross-country day", Some(4)),
                ("Local thermals", Some(2)),
                ("Weak thermals", Some(1)),
                ("Not flyable", None),
            ]
        );
    }

    #[test]
    fn a_map_whose_lists_do_not_fit_its_grid_is_refused() {
        let text = String::from_utf8(body()).unwrap();
        let short = text.replace(r#""ground_m": [612, 1240, null]"#, r#""ground_m": [612]"#);
        assert!(matches!(
            thermal_map(200, short.into_bytes(), 0, NOON),
            Err(CoreError::Server { .. })
        ));
        let no_step = text.replace(r#""step_deg": 0.1"#, r#""step_deg": 0.0"#);
        assert!(matches!(
            thermal_map(200, no_step.into_bytes(), 0, NOON),
            Err(CoreError::Server { .. })
        ));
    }

    #[test]
    fn a_refusal_of_the_server_is_passed_on_in_words() {
        let body =
            br#"{"error": {"code": "not_ready", "message": "The thermal map is not made yet."}}"#;
        assert!(thermal_map(503, body.to_vec(), 0, NOON).is_err());
    }
}
