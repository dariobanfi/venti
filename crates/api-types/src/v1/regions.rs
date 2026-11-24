use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::{ModelId, SourceId, Timestamp};

use super::{Attribution, ThermalReason, format_day};

/// Path of the regions of the forecast by region: their outlines, names
/// and labels, the same until the server's regions are made again.
pub const REGIONS_PATH: &str = "/v1/regions";

/// Path of the forecast by region: one day's level of each region, for the
/// day and for each of its hours.
pub const REGION_FORECAST_PATH: &str = "/v1/regions/forecast";

/// The answer of `GET /v1/regions`: a GeoJSON `FeatureCollection` that a
/// map takes as it is, with two members of its own, `version` and
/// `attributions`.
///
/// The regions are the avalanche warning micro-regions of the Alpine
/// members of the EAWS (CC0 1.0): massifs and groups of valleys as the
/// services split their areas, the Prealps included and the foreland
/// (around Munich, the Swiss Mittelland, the Po plain) not. Each outline is
/// simplified by about 100 m, each on its own, so two neighbours may part
/// or overlap along their border by that much.
///
/// Some 700 kB, and the same until `version` changes: a client keeps it and
/// asks again with `If-None-Match`, or when an answer of
/// `GET /v1/regions/forecast` names another `regions_version`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionsResponse {
    /// `FeatureCollection`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Which set of regions this is, "eaws-regions 439ec676 2026-11-24":
    /// the commit of the EAWS's repository it was made from and the day
    /// that chose the regions current then.
    pub version: String,
    /// The regions, in the order of their identifiers.
    pub features: Vec<RegionFeature>,
    /// The credit for the regions.
    pub attributions: Vec<Attribution>,
}

/// One region, as a GeoJSON `Feature`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionFeature {
    /// `Feature`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Its identifier, names and label.
    pub properties: RegionProperties,
    /// Its outline.
    pub geometry: RegionGeometry,
}

/// What a region is called and where its label goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionProperties {
    /// The EAWS identifier: "AT-07-14-01", "CH-4115", "IT-32-BZ-05-01".
    pub id: String,
    /// Its names.
    pub names: RegionNames,
    /// A point inside it for its label, `[longitude, latitude]`.
    pub label: [f64; 2],
}

/// A region's names, as the EAWS gives them. Where a language has no name
/// of its own the EAWS's fallback stands, most often the English.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RegionNames {
    /// In German.
    pub de: String,
    /// In Italian.
    pub it: String,
    /// In English.
    pub en: String,
    /// In French.
    pub fr: String,
    /// In Slovenian.
    pub sl: String,
}

/// A region's outline, a GeoJSON `MultiPolygon`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionGeometry {
    /// `MultiPolygon`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Its polygons, each an outer ring and its holes, each ring a list of
    /// `[longitude, latitude]` whose last position repeats the first.
    pub coordinates: Vec<Vec<Vec<[f64; 2]>>>,
}

/// A request for the forecast by region of one day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionForecastRequest {
    /// The day, as the thermal map names it (`days[].date` of
    /// `GET /v1/thermals`). `None` for the first day the map has.
    pub date: Option<NaiveDate>,
}

impl RegionForecastRequest {
    /// The path and query string to GET.
    #[must_use]
    pub fn path_and_query(&self) -> String {
        match self.date {
            Some(date) => format!("{REGION_FORECAST_PATH}?date={}", format_day(date)),
            None => REGION_FORECAST_PATH.to_owned(),
        }
    }
}

/// How a region's day or hour looks for thermal flying: five levels, the
/// worst first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionLevel {
    /// The forecast rules it out: rain, too much wind, the cloud base low.
    NotFlyable,
    /// Flyable, but nothing to climb in: glides down.
    GlidesOnly,
    /// Weak or local thermals: maybe to stay up.
    Weak,
    /// Thermals to leave with: cross-country.
    Good,
    /// Strong, high thermals for most of the day.
    VeryGood,
}

/// The level of one region, in a day or an hour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionReading {
    /// The region, by its identifier in `GET /v1/regions`.
    pub id: String,
    /// Its level.
    pub level: RegionLevel,
    /// What speaks against it, where the level is `not_flyable`: the thing
    /// met most often among the take-offs or cells that are not flyable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<ThermalReason>,
    /// In `day` alone: how many of the region's take-offs the level is
    /// from, each judged at its own height. Left out where the region has
    /// fewer than two, and the level is from its cells of the map whose
    /// model ground lies below 2,500 m, each at that ground.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeoffs: Option<u32>,
}

/// The levels of the regions in one hour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionHour {
    /// The moment the hour's values are for, as in `GET /v1/thermals/hours`.
    pub time: Timestamp,
    /// The regions that have a level in this hour.
    pub regions: Vec<RegionReading>,
}

/// The answer of `GET /v1/regions/forecast`.
///
/// One day of the thermal map read region by region: for the day and for
/// each of its daylight hours, the level of each region of
/// `GET /v1/regions`. A region's level is the best one that at least a
/// third of its known take-offs reach, each take-off judged at its own
/// height in its cell of the map; a region with fewer than two take-offs
/// takes its cells whose model ground lies below 2,500 m, each at that
/// ground. It is not flyable only when fewer than a third of them are
/// flyable. A region without a cell of the day, or with only cells higher
/// than that, is not in the lists.
///
/// The same reading of a model as the thermal map's, by the same fixed
/// limits: no statement that any place in a region can be flown. Show the
/// model and its run with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionForecastResponse {
    /// The source the model's values were fetched from.
    pub source: SourceId,
    /// The day, by the sun at the cells.
    pub date: NaiveDate,
    /// Every day the forecast by region has, oldest first: the days of
    /// `GET /v1/thermals`.
    pub dates: Vec<NaiveDate>,
    /// The weather model the day is read from, as the day of
    /// `GET /v1/thermals` names it.
    pub model: ModelId,
    /// When the newest run of that model began that the day holds hours of.
    pub run: Timestamp,
    /// Whether every cell has all of the day's daylight hours.
    pub whole: bool,
    /// When the server worked the map out.
    pub computed_at: Timestamp,
    /// The `version` of `GET /v1/regions` the identifiers are of.
    pub regions_version: String,
    /// The level of each region for the whole day, by the rules a day of
    /// the thermal map is judged by.
    pub day: Vec<RegionReading>,
    /// The day's daylight hours, oldest first.
    pub hours: Vec<RegionHour>,
    /// The credit for the forecast source and for the regions.
    pub attributions: Vec<Attribution>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_forecast_of_a_day_is_asked_for_and_read_back() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 12).unwrap();
        assert_eq!(
            RegionForecastRequest { date: Some(date) }.path_and_query(),
            "/v1/regions/forecast?date=2026-07-12"
        );
        assert_eq!(
            RegionForecastRequest { date: None }.path_and_query(),
            "/v1/regions/forecast"
        );
        let text = r#"{
            "source": "open-meteo", "date": "2026-07-12", "dates": ["2026-07-12", "2026-07-13"],
            "model": "icon-d2", "run": "2026-07-12T03:00:00Z", "whole": true,
            "computed_at": "2026-07-12T04:41:00Z",
            "regions_version": "eaws-regions 439ec676 2026-11-24",
            "day": [
                {"id": "AT-07-14-01", "level": "good", "takeoffs": 3},
                {"id": "CH-4115", "level": "not_flyable", "why": "wind"}
            ],
            "hours": [{"time": "2026-07-12T11:00:00Z", "regions": [
                {"id": "AT-07-14-01", "level": "very_good"}
            ]}],
            "attributions": []
        }"#;
        let forecast: RegionForecastResponse = serde_json::from_str(text).unwrap();
        assert_eq!(forecast.day[0].level, RegionLevel::Good);
        assert_eq!(forecast.day[0].takeoffs, Some(3));
        assert_eq!(forecast.day[1].why, Some(ThermalReason::Wind));
        assert_eq!(forecast.hours[0].regions[0].level, RegionLevel::VeryGood);
        assert_eq!(
            serde_json::to_value(&forecast).unwrap(),
            serde_json::from_str::<serde_json::Value>(text).unwrap()
        );
        // The levels in their order, the worst first.
        assert!(RegionLevel::NotFlyable < RegionLevel::GlidesOnly);
        assert!(RegionLevel::Good < RegionLevel::VeryGood);
    }

    #[test]
    fn the_regions_are_a_feature_collection() {
        let text = r#"{
            "type": "FeatureCollection", "version": "eaws-regions 439ec676 2026-11-24",
            "features": [{
                "type": "Feature",
                "properties": {
                    "id": "DE-BY-41",
                    "names": {"de": "Isarwinkel", "it": "Isarwinkel", "en": "Isarwinkel",
                              "fr": "Isarwinkel", "sl": "Isarwinkel"},
                    "label": [11.5, 47.6]
                },
                "geometry": {"type": "MultiPolygon", "coordinates": [[[
                    [11.4, 47.5], [11.6, 47.5], [11.6, 47.7], [11.4, 47.5]
                ]]]}
            }],
            "attributions": []
        }"#;
        let regions: RegionsResponse = serde_json::from_str(text).unwrap();
        assert_eq!(regions.features[0].properties.id, "DE-BY-41");
        assert_eq!(
            serde_json::to_value(&regions).unwrap(),
            serde_json::from_str::<serde_json::Value>(text).unwrap()
        );
    }
}
