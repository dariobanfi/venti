use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, ModelId, SourceId, Timestamp};

use super::Attribution;

/// Path of the thermal map. It takes no parameters: the answer is the map
/// of the whole area the server has forecasts for.
pub const THERMALS_PATH: &str = "/v1/thermals";

/// The step of the thermal map's grid, in degrees of latitude and of
/// longitude: some 11 km north to south and 7.5 km east to west in the
/// Alps. It is a sample, one of the model's cells out of the twenty or so
/// that lie in each square, coarse enough to be asked for a whole region
/// with every run of the model.
pub const THERMAL_GRID_DEG: f64 = 0.1;

/// The answer of `GET /v1/thermals`.
///
/// What a weather model makes of the thermals of each day, for every cell
/// of a grid over the area the server has forecasts for: whether the day
/// can be flown by the air alone, what kind of day its thermals make, the
/// climb to expect and how high it goes. It is the server's own estimate
/// from the model's heat, temperatures and wind, by fixed limits, and the
/// same one an app works out for a take-off from `GET /v1/forecast`;
/// a cell is judged at the height of the model's ground there, a take-off
/// at its own.
///
/// A reading of a model for a square some ten kilometres wide: no
/// statement that any place in it can be flown. Show the model and its
/// run with it.
///
/// A day holds its values in lists that are as long as the grid has cells
/// and in the grid's order ([`ThermalGrid`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermalMapResponse {
    /// The source the model's values were fetched from. Its credit is in
    /// `attributions`.
    pub source: SourceId,
    /// The weather model.
    pub model: ModelId,
    /// When the newest run began that the map holds hours of. Earlier
    /// hours of a day can be an earlier run's: a run starts at its own
    /// hour and says nothing of the morning before it.
    pub run: Timestamp,
    /// When the server worked the map out.
    pub computed_at: Timestamp,
    /// Where the cells are.
    pub grid: ThermalGrid,
    /// The height of the model's ground at each cell, metres above mean
    /// sea level, which is the height the cell is judged at. `null` where
    /// the model's answer for the cell is missing.
    pub ground_m: Vec<Option<i32>>,
    /// The days the map has, oldest first.
    pub days: Vec<ThermalMapDay>,
    /// The credit for the source, with what its licence asks a client to
    /// show.
    pub attributions: Vec<Attribution>,
}

/// The grid of a thermal map.
///
/// Cell number `row * columns + column` is the square of `step_deg`
/// degrees around the point at latitude `south + row * step_deg` and
/// longitude `west + column * step_deg`: the first cell is the one in the
/// south-west, cells run west to east, rows south to north. The model's
/// values are those of its own cell nearest that point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThermalGrid {
    /// The latitude of the points of the southernmost row, degrees.
    pub south: f64,
    /// The longitude of the points of the westernmost column, degrees.
    pub west: f64,
    /// The distance between neighbouring points, degrees, in both
    /// directions.
    pub step_deg: f64,
    /// How many rows the grid has.
    pub rows: u32,
    /// How many columns.
    pub columns: u32,
}

impl ThermalGrid {
    /// How many cells the grid has.
    #[must_use]
    pub fn cells(&self) -> usize {
        self.rows as usize * self.columns as usize
    }

    /// The point in the middle of a cell, by its number. `None` for a
    /// number the grid does not have, and for a grid that does not lie on
    /// the globe.
    #[must_use]
    pub fn point(&self, cell: usize) -> Option<GeoPoint> {
        if cell >= self.cells() {
            return None;
        }
        let columns = self.columns as usize;
        let (row, column) = (cell / columns, cell % columns);
        GeoPoint::new(
            self.south + row as f64 * self.step_deg,
            self.west + column as f64 * self.step_deg,
        )
        .ok()
    }

    /// The number of the cell a place lies in. `None` outside the grid.
    #[must_use]
    pub fn cell_at(&self, place: GeoPoint) -> Option<usize> {
        if self.step_deg <= 0.0 {
            return None;
        }
        let row = ((place.lat() - self.south) / self.step_deg).round();
        let column = ((place.lon() - self.west) / self.step_deg).round();
        let inside = row >= 0.0
            && column >= 0.0
            && row < f64::from(self.rows)
            && column < f64::from(self.columns);
        inside.then(|| row as usize * self.columns as usize + column as usize)
    }
}

/// One day of a thermal map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermalMapDay {
    /// The day, by the sun at the cells: from one midnight by the sun to
    /// the next.
    pub date: NaiveDate,
    /// Whether every cell that has a verdict was judged with all of the
    /// day's daylight hours. `false` on a day the model's runs at hand do
    /// not reach the whole of: the day they end in, and today after a
    /// start of the server. Such a day is judged by the hours there are,
    /// which can make it look worse than it is.
    pub whole: bool,
    /// For each cell, whether the day can be flown by the air alone.
    /// `null` where the day has no hours for the cell.
    pub verdict: Vec<Option<ThermalVerdict>>,
    /// For each cell, what speaks against the day: the thing met most
    /// often among the hours that are no better than the day's verdict.
    /// `null` on a flyable day and where the day has no hours. Left out
    /// by a server older than this field; a client then has no reason to
    /// show.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub why: Vec<Option<ThermalReason>>,
    /// For each cell, the kind of day its thermals make. `null` on a day
    /// that is not flyable, and where there is no estimate.
    pub thermals: Vec<Option<ThermalKind>>,
    /// For each cell, the climb to expect in the day's thermal hours, m/s
    /// to a tenth: the mean over the hours that make the kind of day, or
    /// the best hour's on a day of weak thermals. `null` where `thermals`
    /// is.
    pub climb_ms: Vec<Option<f64>>,
    /// For each cell, how high the thermals go, metres above mean sea
    /// level to the nearest fifty. `null` without thermals.
    pub top_m: Vec<Option<i32>>,
    /// For each cell, whether that height is a base of cumulus; `false`
    /// in the blue and without thermals.
    pub cloud: Vec<bool>,
}

/// Whether a day can be flown, by a forecast of the air alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalVerdict {
    /// Nothing in the forecast speaks against it.
    Flyable,
    /// Something does: wind, gusts, a low cloud base or air in which
    /// showers can grow.
    Marginal,
    /// The forecast rules it out: rain, or too much wind.
    NotFlyable,
}

/// What a forecast has against a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalReason {
    /// Rain or snow.
    Rain,
    /// The mean wind 10 m above the model's ground.
    Wind,
    /// The gusts there.
    Gusts,
    /// The wind at the model's levels above the ground.
    WindAloft,
    /// Convective cloud with its base at or just above the ground.
    CloudBase,
    /// Air in which showers or thunderstorms can grow.
    Storms,
}

/// The kind of day a forecast's thermals make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalKind {
    /// None a paraglider climbs in, or none that reach above the ground.
    Weak,
    /// Enough to stay up in.
    Local,
    /// Strong and high enough to leave with.
    CrossCountry,
    /// Strong, high and for most of the day.
    Big,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> ThermalGrid {
        ThermalGrid {
            south: 47.0,
            west: 11.0,
            step_deg: 0.1,
            rows: 2,
            columns: 3,
        }
    }

    #[test]
    fn cells_run_west_to_east_and_rows_south_to_north() {
        let grid = grid();
        assert_eq!(grid.cells(), 6);
        let point = |cell| grid.point(cell).map(|point| (point.lat(), point.lon()));
        assert_eq!(point(0), Some((47.0, 11.0)));
        let (lat, lon) = point(5).unwrap();
        assert!((lat - 47.1).abs() < 1e-9 && (lon - 11.2).abs() < 1e-9);
        assert_eq!(point(6), None);
    }

    #[test]
    fn a_place_lies_in_the_cell_whose_point_is_nearest() {
        let grid = grid();
        let cell = |lat, lon| grid.cell_at(GeoPoint::new(lat, lon).unwrap());
        assert_eq!(cell(47.0, 11.0), Some(0));
        assert_eq!(cell(47.04, 11.14), Some(1));
        assert_eq!(cell(47.06, 11.24), Some(5));
        // Half a step outside the outermost points is still their cell.
        assert_eq!(cell(46.96, 10.96), Some(0));
        assert_eq!(cell(46.94, 11.0), None);
        assert_eq!(cell(47.0, 11.26), None);
    }

    #[test]
    fn a_map_is_written_in_plain_words_and_read_back() {
        let text = r#"{
            "source": "open-meteo", "model": "icon-d2",
            "run": "2026-07-12T03:00:00Z", "computed_at": "2026-07-12T04:41:00Z",
            "grid": {"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": 1, "columns": 3},
            "ground_m": [612, 1240, null],
            "days": [{
                "date": "2026-07-12", "whole": true,
                "verdict": ["flyable", "not_flyable", null],
                "why": [null, "wind_aloft", null],
                "thermals": ["cross_country", null, null],
                "climb_ms": [1.8, null, null],
                "top_m": [3250, null, null],
                "cloud": [true, false, false]
            }],
            "attributions": []
        }"#;

        let map: ThermalMapResponse = serde_json::from_str(text).unwrap();

        assert_eq!(map.model.as_str(), "icon-d2");
        let day = &map.days[0];
        assert_eq!(day.date, NaiveDate::from_ymd_opt(2026, 7, 12).unwrap());
        assert_eq!(day.verdict[1], Some(ThermalVerdict::NotFlyable));
        assert_eq!(day.why[1], Some(ThermalReason::WindAloft));
        assert_eq!(day.thermals[0], Some(ThermalKind::CrossCountry));
        assert_eq!(day.top_m[0], Some(3250));
        let written = serde_json::to_value(&map).unwrap();
        assert_eq!(
            written,
            serde_json::from_str::<serde_json::Value>(text).unwrap()
        );

        // An answer of a server that knows no reasons is read as one
        // without any.
        let older = text.replace(r#""why": [null, "wind_aloft", null],"#, "");
        let map: ThermalMapResponse = serde_json::from_str(&older).unwrap();
        assert!(map.days[0].why.is_empty());
    }
}
