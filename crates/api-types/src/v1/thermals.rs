use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, ModelId, SourceId, Timestamp};

use super::{Attribution, format_day};

/// Path of the thermal map. It takes no parameters: the answer is the map
/// of the whole area the server has forecasts for.
pub const THERMALS_PATH: &str = "/v1/thermals";

/// Path of the hours of one day of the thermal map.
pub const THERMAL_HOURS_PATH: &str = "/v1/thermals/hours";

/// What of a day's hours an answer of `GET /v1/thermals/hours` holds: a
/// day of a region hour by hour is too much for one answer, so it comes a
/// layer at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalLayer {
    /// Each hour's verdict and what its thermals count as: what the days
    /// of `GET /v1/thermals` are made of.
    Thermals,
    /// The wind: 10 m above the model's ground with its gusts, or at one
    /// height above mean sea level.
    Wind,
    /// The share of the sky covered by cloud.
    Cloud,
    /// The rain, showers and snow of each hour.
    Rain,
}

impl ThermalLayer {
    /// Every layer, in the order the documentation lists them.
    pub const ALL: [Self; 4] = [Self::Thermals, Self::Wind, Self::Cloud, Self::Rain];

    /// The layer as the `layer` parameter names it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thermals => "thermals",
            Self::Wind => "wind",
            Self::Cloud => "cloud",
            Self::Rain => "rain",
        }
    }

    /// The layer a `layer` parameter names. `None` for any other text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|layer| layer.as_str() == text)
    }
}

/// A request for one layer of the hours of one day of the thermal map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalHoursRequest {
    /// The day, as the map names it (`days[].date` of `GET /v1/thermals`).
    pub date: NaiveDate,
    /// The layer.
    pub layer: ThermalLayer,
    /// For the wind: the height above mean sea level to read it at,
    /// metres, one of the answer's `heights_m`. `None` for the wind 10 m
    /// above the model's ground, and for every other layer.
    pub height_m: Option<i32>,
}

impl ThermalHoursRequest {
    /// The path and query string to GET.
    #[must_use]
    pub fn path_and_query(&self) -> String {
        let mut path = format!(
            "{THERMAL_HOURS_PATH}?date={}&layer={}",
            format_day(self.date),
            self.layer.as_str()
        );
        if let Some(height) = self.height_m {
            path.push_str(&format!("&height={height}"));
        }
        path
    }
}

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
    /// The finest weather model the map is read from. A day can be read
    /// from a coarser one: each day names its own (`days[].model`).
    pub model: ModelId,
    /// When the newest run of that model began that the map holds hours
    /// of. Earlier hours of a day can be an earlier run's: a run starts at
    /// its own hour and says nothing of the morning before it.
    pub run: Timestamp,
    /// When the server worked the map out.
    pub computed_at: Timestamp,
    /// Where the cells are.
    pub grid: ThermalGrid,
    /// The height of the model's ground at each cell, metres above mean
    /// sea level, which is the height the cell is judged at. `null` where
    /// the model's answer for the cell is missing.
    pub ground_m: Vec<Option<i32>>,
    /// The heights above mean sea level at which
    /// `GET /v1/thermals/hours` gives the wind of each of the map's days,
    /// metres, the lowest first. Left out by a server older than the
    /// hours: such a server has no hours to ask for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heights_m: Vec<i32>,
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
    /// The weather model the day is read from. A day is one model's, never
    /// a blend: the finest model's, unless a coarser one has more of the
    /// day's hours, which is the day the finest model's run ends in and
    /// the days after it. Left out by a server older than this field: the
    /// day is then the answer's `model`'s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    /// When the newest run of that model began that the day holds hours
    /// of. Left out where `model` is: the answer's `run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Timestamp>,
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

/// The answer of `GET /v1/thermals/hours`.
///
/// One layer of one day of the thermal map, hour by hour: for every
/// daylight hour of the day and every cell of the grid, the hour's verdict
/// and what its thermals count as, as the day of `GET /v1/thermals` is
/// made of them, or what the model has in that hour of wind, of cloud or
/// of rain. An hour holds the lists of the layer that was asked for and
/// leaves the others out.
///
/// The hours are the day's daylight hours, as the map judges them: from an
/// hour after sunrise to an hour before sunset, by the sun at each cell.
/// The night is not in the answer.
///
/// Every list of an hour is as long as the grid has cells and in the
/// grid's order ([`ThermalGrid`]). Wind speeds are whole km/h, directions
/// whole degrees to the nearest ten, cloud whole percent and rain
/// millimetres to a tenth: what a map shows, in whole numbers, which keeps
/// a layer of a region's day to a size a phone fetches.
/// `GET /v1/forecast` has the model's own figures for one place.
///
/// Like the map of the days it is a reading of a model for a square some
/// ten kilometres wide, one of the model's cells out of the twenty or so
/// in it: a valley's own wind is not in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermalHoursResponse {
    /// The source the model's values were fetched from. Its credit is in
    /// `attributions`.
    pub source: SourceId,
    /// The day, by the sun at the cells.
    pub date: NaiveDate,
    /// The layer the hours hold.
    pub layer: ThermalLayer,
    /// For the wind at a height above mean sea level: that height, metres.
    /// Left out for the wind 10 m above the model's ground and for every
    /// other layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_m: Option<i32>,
    /// The heights above mean sea level the wind can be asked for at,
    /// metres, the lowest first. The same in every answer.
    pub heights_m: Vec<i32>,
    /// The weather model the day is read from: the one the day of
    /// `GET /v1/thermals` names. Every hour of the answer is this model's.
    pub model: ModelId,
    /// When the newest run of that model began that the day holds hours
    /// of. Earlier hours of the day can be an earlier run's.
    pub run: Timestamp,
    /// Whether every cell has all of the day's daylight hours, as
    /// `days[].whole` of `GET /v1/thermals`.
    pub whole: bool,
    /// When the server worked the map out.
    pub computed_at: Timestamp,
    /// Where the cells are.
    pub grid: ThermalGrid,
    /// The height of this model's ground at each cell, metres above mean
    /// sea level: the height the cell's hours are judged at, 10 m under
    /// its wind at the ground, and what a height has to lie above to have
    /// a wind. `null` where the model gives none, as for a cell of water.
    pub ground_m: Vec<Option<i32>>,
    /// The day's daylight hours that any cell has, oldest first.
    pub hours: Vec<ThermalMapHour>,
    /// The credit for the source, with what its licence asks a client to
    /// show.
    pub attributions: Vec<Attribution>,
}

/// One hour of a day of the thermal map: the lists of one layer.
///
/// A cell has `null` in every list of an hour that is none of its own
/// daylight hours, or that the model's runs at hand do not hold for it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ThermalMapHour {
    /// The moment the hour's values are for. Rain and gusts are those of
    /// the hour before it, as the model gives them.
    pub time: Timestamp,
    /// Layer `thermals`. For each cell, whether the hour can be flown by
    /// the air alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verdict: Vec<Option<ThermalVerdict>>,
    /// Layer `thermals`. For each cell, the worst thing the air has
    /// against the hour. `null` in a flyable hour.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub why: Vec<Option<ThermalReason>>,
    /// Layer `thermals`. For each cell, what the hour's thermals count
    /// as. `null` in an hour that is not flyable, and where there is no
    /// estimate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thermals: Vec<Option<ThermalKind>>,
    /// Layer `thermals`. For each cell, the climb to expect in the hour's
    /// thermals, m/s to a tenth. `null` where `thermals` is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub climb_ms: Vec<Option<f64>>,
    /// Layer `thermals`. For each cell, how high the thermals go, metres
    /// above mean sea level to the nearest fifty. `null` without thermals.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_m: Vec<Option<i32>>,
    /// Layer `thermals`. For each cell, whether that height is a base of
    /// cumulus; `false` in the blue and without thermals.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloud: Vec<bool>,
    /// Layer `wind`. For each cell, the wind's mean speed, whole km/h:
    /// 10 m above the model's ground, or at the answer's `height_m`.
    ///
    /// At a height the wind is read between the model's levels
    /// (`venti_flight::aloft`): the speed in a straight line by height
    /// between the level below and the level above, the direction along
    /// the shorter way round. It is `null` where the height lies under
    /// the model's ground (`ground_m` says how high that is), above the
    /// model's highest level, or where the model gives no height of its
    /// ground. Nothing is read beyond the levels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wind_kmh: Vec<Option<u16>>,
    /// Layer `wind`. For each cell, the direction that wind comes from,
    /// degrees to the nearest ten, from 0 to 350.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wind_from_deg: Vec<Option<u16>>,
    /// Layer `wind`, 10 m above the model's ground only. For each cell,
    /// the strongest gust there in the hour, whole km/h.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gust_kmh: Vec<Option<u16>>,
    /// Layer `cloud`. For each cell, the share of the sky covered by
    /// cloud at any height, whole percent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloud_cover_pct: Vec<Option<u8>>,
    /// Layer `rain`. For each cell, the rain, showers and snow of the
    /// hour, millimetres of water to a tenth.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub precipitation_mm: Vec<Option<f64>>,
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
    fn the_hours_of_a_day_are_asked_for_by_date_and_layer_and_read_back() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 12).unwrap();
        let request = |layer, height_m| {
            ThermalHoursRequest {
                date,
                layer,
                height_m,
            }
            .path_and_query()
        };
        assert_eq!(
            request(ThermalLayer::Thermals, None),
            "/v1/thermals/hours?date=2026-07-12&layer=thermals"
        );
        assert_eq!(
            request(ThermalLayer::Wind, Some(2000)),
            "/v1/thermals/hours?date=2026-07-12&layer=wind&height=2000"
        );
        for layer in ThermalLayer::ALL {
            assert_eq!(ThermalLayer::parse(layer.as_str()), Some(layer));
            // The parameter and the answer name a layer alike.
            assert_eq!(
                serde_json::to_value(layer).unwrap(),
                serde_json::Value::from(layer.as_str())
            );
        }
        assert_eq!(ThermalLayer::parse("Wind"), None);

        // The wind at 1000 m: the second cell's ground lies above it.
        let text = r#"{
            "source": "open-meteo", "date": "2026-07-12",
            "layer": "wind", "height_m": 1000, "heights_m": [1000, 2000],
            "model": "icon-d2", "run": "2026-07-12T03:00:00Z", "whole": true,
            "computed_at": "2026-07-12T04:41:00Z",
            "grid": {"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": 1, "columns": 2},
            "ground_m": [612, 1240],
            "hours": [{
                "time": "2026-07-12T11:00:00Z",
                "wind_kmh": [14, null],
                "wind_from_deg": [280, null]
            }],
            "attributions": []
        }"#;
        let hours: ThermalHoursResponse = serde_json::from_str(text).unwrap();
        assert_eq!(
            (hours.layer, hours.height_m),
            (ThermalLayer::Wind, Some(1000))
        );
        let hour = &hours.hours[0];
        assert_eq!(hour.wind_kmh, [Some(14), None]);
        assert_eq!(hour.wind_from_deg, [Some(280), None]);
        // The lists of the other layers are left out, and read as empty.
        assert!(hour.gust_kmh.is_empty() && hour.verdict.is_empty());
        let written = serde_json::to_value(&hours).unwrap();
        assert_eq!(
            written,
            serde_json::from_str::<serde_json::Value>(text).unwrap()
        );

        // The layer of the hours' verdicts.
        let text = text
            .replace(
                r#""layer": "wind", "height_m": 1000,"#,
                r#""layer": "thermals","#,
            )
            .replace(
                r#""wind_kmh": [14, null],
                "wind_from_deg": [280, null]"#,
                r#""verdict": ["flyable", "marginal"], "why": [null, "gusts"],
                "thermals": ["cross_country", "local"], "climb_ms": [1.8, 0.7],
                "top_m": [3250, 2100], "cloud": [true, false]"#,
            );
        let hours: ThermalHoursResponse = serde_json::from_str(&text).unwrap();
        assert_eq!(
            (hours.layer, hours.height_m),
            (ThermalLayer::Thermals, None)
        );
        let hour = &hours.hours[0];
        assert_eq!(hour.verdict[1], Some(ThermalVerdict::Marginal));
        assert_eq!(hour.why[1], Some(ThermalReason::Gusts));
        assert_eq!(hour.thermals[0], Some(ThermalKind::CrossCountry));
        assert_eq!((hour.climb_ms[0], hour.top_m[0]), (Some(1.8), Some(3250)));
        assert!(hour.wind_kmh.is_empty());
        assert_eq!(
            serde_json::to_value(&hours).unwrap(),
            serde_json::from_str::<serde_json::Value>(&text).unwrap()
        );
    }

    #[test]
    fn a_map_is_written_in_plain_words_and_read_back() {
        let text = r#"{
            "source": "open-meteo", "model": "icon-d2",
            "run": "2026-07-12T03:00:00Z", "computed_at": "2026-07-12T04:41:00Z",
            "grid": {"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": 1, "columns": 3},
            "ground_m": [612, 1240, null],
            "days": [{
                "date": "2026-07-12", "model": "icon-eu", "run": "2026-07-12T00:00:00Z",
                "whole": true,
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
        assert_eq!(day.model.as_ref().map(ModelId::as_str), Some("icon-eu"));
        assert_eq!(
            day.run.map(|run| run.to_rfc3339()).as_deref(),
            Some("2026-07-12T00:00:00+00:00")
        );
        assert_eq!(day.verdict[1], Some(ThermalVerdict::NotFlyable));
        assert_eq!(day.why[1], Some(ThermalReason::WindAloft));
        assert_eq!(day.thermals[0], Some(ThermalKind::CrossCountry));
        assert_eq!(day.top_m[0], Some(3250));
        let written = serde_json::to_value(&map).unwrap();
        assert_eq!(
            written,
            serde_json::from_str::<serde_json::Value>(text).unwrap()
        );

        // The sample is of a server older than the hours: it names no
        // heights. One that serves them does, before the days.
        assert!(map.heights_m.is_empty());
        let newer = text.replace(r#""days": [{"#, r#""heights_m": [1000, 2000], "days": [{"#);
        let map: ThermalMapResponse = serde_json::from_str(&newer).unwrap();
        assert_eq!(map.heights_m, [1000, 2000]);

        // An answer of a server that knows no reasons is read as one
        // without any.
        let older = text.replace(r#""why": [null, "wind_aloft", null],"#, "");
        let map: ThermalMapResponse = serde_json::from_str(&older).unwrap();
        assert!(map.days[0].why.is_empty());

        // One that names no model for a day: the day is the answer's.
        let older = text.replace(r#""model": "icon-eu", "run": "2026-07-12T00:00:00Z","#, "");
        let map: ThermalMapResponse = serde_json::from_str(&older).unwrap();
        assert_eq!((map.days[0].model.clone(), map.days[0].run), (None, None));
    }
}
