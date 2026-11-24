//! The thermal forecast by region: the thermal map read region by region,
//! as the server hands it out (`GET /v1/regions/forecast`), over the
//! regions it hands out once (`GET /v1/regions`).
//!
//! A shell fetches the regions once ([`regions_path`]), hands the answer
//! to [`ForecastRegions::read`] and gives its map the outlines and the
//! labels as they are ([`ForecastRegions::outlines`],
//! [`ForecastRegions::labels`]): GeoJSON, each feature with its `id`. For
//! a day it fetches [`region_forecast_path`] and reads it with
//! [`RegionForecast::read`]; for the day and for each hour that gives a
//! list of regions with their level and its words, which the shell colours
//! its regions by, matched on `id`. A region not in a list has no level
//! and is not filled.
//!
//! **The colours** of the five levels are tokens of the design language
//! still to come (owner decision 27): the core names the levels and their
//! words, and the shell takes the colour of a level from the tokens.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use venti_api_types::v1;

use crate::server::answer;
use crate::thermal_hours::{ThermalMapHourMark, first_of, hour_mark, lower_first};
use crate::thermal_map::words_of;
use crate::wind::credit;
use crate::{CoreError, Credit};

/// How a region's day or hour looks for thermal flying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum RegionLevel {
    /// The forecast rules it out.
    NotFlyable,
    /// Flyable, nothing to climb in.
    GlidesOnly,
    /// Weak or local thermals.
    Weak,
    /// Thermals to leave with.
    Good,
    /// Strong, high thermals for most of the day.
    VeryGood,
}

impl RegionLevel {
    /// The five, the best first: the order of a key.
    const ALL: [Self; 5] = [
        Self::VeryGood,
        Self::Good,
        Self::Weak,
        Self::GlidesOnly,
        Self::NotFlyable,
    ];

    /// What to call it: "Very good".
    fn name(self) -> &'static str {
        match self {
            Self::NotFlyable => "Not flyable",
            Self::GlidesOnly => "Glides only",
            Self::Weak => "Weak",
            Self::Good => "Good",
            Self::VeryGood => "Very good",
        }
    }

    /// What it means, for a key: "Thermals to leave with".
    fn words(self) -> &'static str {
        match self {
            Self::NotFlyable => "The forecast rules it out",
            Self::GlidesOnly => "Flyable, nothing to climb in",
            Self::Weak => "Weak thermals, maybe to stay up",
            Self::Good => "Thermals to leave with",
            Self::VeryGood => "Strong, high thermals for most of the day",
        }
    }

    fn of(level: v1::RegionLevel) -> Self {
        match level {
            v1::RegionLevel::NotFlyable => Self::NotFlyable,
            v1::RegionLevel::GlidesOnly => Self::GlidesOnly,
            v1::RegionLevel::Weak => Self::Weak,
            v1::RegionLevel::Good => Self::Good,
            v1::RegionLevel::VeryGood => Self::VeryGood,
        }
    }
}

/// One line of the key of the forecast by region.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RegionLevelKey {
    /// The level, whose colour the shell takes from the design tokens.
    pub level: RegionLevel,
    /// Its name: "Good".
    pub name: String,
    /// What it means: "Thermals to leave with".
    pub words: String,
}

/// The key of the forecast by region, the best level first.
#[uniffi::export]
#[must_use]
pub fn region_levels() -> Vec<RegionLevelKey> {
    RegionLevel::ALL
        .into_iter()
        .map(|level| RegionLevelKey {
            level,
            name: level.name().to_owned(),
            words: level.words().to_owned(),
        })
        .collect()
}

/// The path to `GET` from the server for the regions.
#[uniffi::export]
#[must_use]
pub fn regions_path() -> String {
    v1::REGIONS_PATH.to_owned()
}

/// The path and query to `GET` for the forecast by region of a day, as the
/// thermal map names it ([`crate::ThermalMapDay::date`]); none for today.
#[uniffi::export]
pub fn region_forecast_path(date: Option<String>) -> Result<String, CoreError> {
    let date = date
        .map(|date| {
            v1::parse_day(&date).map_err(|_| CoreError::Server {
                message: "The app could not ask the server for this day.".to_owned(),
            })
        })
        .transpose()?;
    Ok(v1::RegionForecastRequest { date }.path_and_query())
}

/// The regions, read once: what a map draws them with.
#[derive(Debug, uniffi::Object)]
pub struct ForecastRegions {
    version: String,
    outlines: String,
    labels: String,
    names: HashMap<String, String>,
    credits: Vec<Credit>,
}

#[uniffi::export]
impl ForecastRegions {
    /// Reads the server's answer to [`regions_path`]. `language` is the
    /// language of the names: "de", "it", "en", "fr" or "sl"; any other
    /// is English.
    #[uniffi::constructor]
    pub fn read(status: u16, body: Vec<u8>, language: String) -> Result<Arc<Self>, CoreError> {
        let regions: v1::RegionsResponse = answer(status, &body)?;
        let name = |names: &v1::RegionNames| -> String {
            let chosen = match language.as_str() {
                "de" => &names.de,
                "it" => &names.it,
                "fr" => &names.fr,
                "sl" => &names.sl,
                _ => &names.en,
            };
            if chosen.is_empty() {
                names.en.clone()
            } else {
                chosen.clone()
            }
        };
        let mut outlines = Vec::with_capacity(regions.features.len());
        let mut labels = Vec::with_capacity(regions.features.len());
        let mut names = HashMap::with_capacity(regions.features.len());
        for feature in &regions.features {
            let properties = &feature.properties;
            let called = name(&properties.names);
            outlines.push(serde_json::json!({
                "type": "Feature",
                "properties": {"id": properties.id, "name": called},
                "geometry": {"type": "MultiPolygon", "coordinates": feature.geometry.coordinates},
            }));
            labels.push(serde_json::json!({
                "type": "Feature",
                "properties": {"id": properties.id, "name": called},
                "geometry": {"type": "Point", "coordinates": properties.label},
            }));
            names.insert(properties.id.clone(), called);
        }
        let collection = |features: Vec<serde_json::Value>| {
            serde_json::json!({"type": "FeatureCollection", "features": features}).to_string()
        };
        Ok(Arc::new(Self {
            version: regions.version,
            outlines: collection(outlines),
            labels: collection(labels),
            names,
            credits: regions.attributions.iter().map(credit).collect(),
        }))
    }

    /// Which set of regions this is: what a forecast's `regions_version`
    /// is held to ([`RegionForecastView::regions_version`]).
    pub fn version(&self) -> String {
        self.version.clone()
    }

    /// The outlines, a GeoJSON `FeatureCollection` of `MultiPolygon`s,
    /// each with `id` and `name`: a fill and a line on the map, coloured
    /// by the level of the region the `id` names.
    pub fn outlines(&self) -> String {
        self.outlines.clone()
    }

    /// The labels, a GeoJSON `FeatureCollection` of `Point`s inside the
    /// regions, each with `id` and `name`.
    pub fn labels(&self) -> String {
        self.labels.clone()
    }

    /// A region's name, by its `id`.
    pub fn name_of(&self, id: String) -> Option<String> {
        self.names.get(&id).cloned()
    }

    /// The credits to show wherever the regions are: "Regions: EAWS
    /// (CC0)".
    pub fn credits(&self) -> Vec<Credit> {
        self.credits.clone()
    }
}

/// A region's level, in a day or an hour.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RegionReadingView {
    /// The region, as the outlines name it.
    pub id: String,
    /// Its level.
    pub level: RegionLevel,
    /// The level in words, with what speaks against it where it is not
    /// flyable: "Good", "Not flyable: wind".
    pub words: String,
    /// What it is judged from, for the day: "From 4 take-offs", "From the
    /// model's ground". Empty for an hour.
    pub detail: String,
}

/// What a card says of a day of the forecast by region.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RegionForecastView {
    /// The day: "2026-07-12".
    pub date: String,
    /// Every day there is a forecast by region of, oldest first.
    pub dates: Vec<String>,
    /// The day's daylight hours, the stops of the line of hours; the
    /// number of an hour is its place here, from 0.
    pub hours: Vec<ThermalMapHourMark>,
    /// The hour to show first: today the hour that has begun, on another
    /// day the one nearest one o'clock in the afternoon.
    pub first_hour: u32,
    /// The weather model: "ICON-D2".
    pub model: String,
    /// When the newest run of it began, seconds since 1970.
    pub run_unix_s: i64,
    /// Whether every square has all of the day's daylight hours.
    pub whole: bool,
    /// What to say under the line of hours when the day is not whole;
    /// empty when it is.
    pub note: String,
    /// The set of regions the forecast is of: another than
    /// [`ForecastRegions::version`] means the regions are to be fetched
    /// again.
    pub regions_version: String,
    /// The credits to show wherever the forecast is: the forecast
    /// source's and the regions'.
    pub credits: Vec<Credit>,
}

/// A day of the forecast by region: the server's answer, read once and
/// asked for the day and hour by hour.
#[derive(Debug, uniffi::Object)]
pub struct RegionForecast {
    response: v1::RegionForecastResponse,
    utc_offset_s: i32,
    first_hour: u32,
}

/// A reading in words.
fn reading(reading: &v1::RegionReading) -> RegionReadingView {
    let level = RegionLevel::of(reading.level);
    let words = match (level, reading.why) {
        (RegionLevel::NotFlyable, Some(why)) => {
            format!("{}: {}", level.name(), lower_first(words_of(why)))
        }
        _ => level.name().to_owned(),
    };
    RegionReadingView {
        id: reading.id.clone(),
        level,
        words,
        detail: String::new(),
    }
}

#[uniffi::export]
impl RegionForecast {
    /// Reads the server's answer to [`region_forecast_path`].
    /// `utc_offset_s` is how far the pilot's clock is ahead of UTC, which
    /// labels the hours, and `now_unix_s` the moment by Venti's clock,
    /// which decides the hour to show first; zero or less shows the hour
    /// nearest one o'clock in the afternoon first.
    #[uniffi::constructor]
    pub fn read(
        status: u16,
        body: Vec<u8>,
        utc_offset_s: i32,
        now_unix_s: i64,
    ) -> Result<Arc<Self>, CoreError> {
        let response: v1::RegionForecastResponse = answer(status, &body)?;
        let times: Vec<DateTime<Utc>> = response.hours.iter().map(|hour| hour.time).collect();
        let first_hour = first_of(response.date, &times, utc_offset_s, now_unix_s);
        Ok(Arc::new(Self {
            response,
            utc_offset_s,
            first_hour,
        }))
    }

    /// What a card says of the day: its hours, the hour to show first,
    /// the model and its run.
    pub fn view(&self) -> RegionForecastView {
        let response = &self.response;
        RegionForecastView {
            date: response.date.format("%Y-%m-%d").to_string(),
            dates: response
                .dates
                .iter()
                .map(|date| date.format("%Y-%m-%d").to_string())
                .collect(),
            hours: response
                .hours
                .iter()
                .map(|hour| hour_mark(hour.time, self.utc_offset_s))
                .collect(),
            first_hour: self.first_hour,
            model: crate::forecast::model_name(response.model.as_str()),
            run_unix_s: response.run.timestamp(),
            whole: response.whole,
            note: if response.whole {
                String::new()
            } else {
                "Not every hour of this day is forecast yet.".to_owned()
            },
            regions_version: response.regions_version.clone(),
            credits: response.attributions.iter().map(credit).collect(),
        }
    }

    /// Each region's level for the whole day.
    pub fn day(&self) -> Vec<RegionReadingView> {
        self.response
            .day
            .iter()
            .map(|of_day| RegionReadingView {
                detail: match of_day.takeoffs {
                    Some(count) => format!("From {count} take-offs"),
                    None => "From the model's ground".to_owned(),
                },
                ..reading(of_day)
            })
            .collect()
    }

    /// Each region's level in one hour, by the hour's number in
    /// [`RegionForecastView::hours`]. Empty for an hour there is none of.
    pub fn hour(&self, hour: u32) -> Vec<RegionReadingView> {
        self.response
            .hours
            .get(hour as usize)
            .map(|hour| hour.regions.iter().map(reading).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOON: i64 = 1_783_857_600; // 2026-07-12 12:00 UTC

    fn forecast() -> Vec<u8> {
        br#"{
            "source": "open-meteo", "date": "2026-07-12", "dates": ["2026-07-12", "2026-07-13"],
            "model": "icon-d2", "run": "2026-07-12T03:00:00Z", "whole": true,
            "computed_at": "2026-07-12T04:41:00Z",
            "regions_version": "eaws-regions 439ec676 2026-11-24",
            "day": [
                {"id": "DE-BY-41", "level": "good", "takeoffs": 3},
                {"id": "CH-4115", "level": "not_flyable", "why": "wind_aloft"}
            ],
            "hours": [
                {"time": "2026-07-12T10:00:00Z", "regions": [{"id": "DE-BY-41", "level": "weak"}]},
                {"time": "2026-07-12T11:00:00Z", "regions": [{"id": "DE-BY-41", "level": "very_good"}]}
            ],
            "attributions": [
                {"source": "open-meteo", "provider": "Open-Meteo", "licence": "CC BY 4.0",
                 "text": "Weather data by Open-Meteo.com", "url": "https://open-meteo.com/"},
                {"source": "eaws-regions", "provider": "EAWS", "licence": "CC0 1.0",
                 "text": "Regions: EAWS (CC0)",
                 "url": "https://gitlab.com/eaws/eaws-regions/-/blob/master/LICENSE"}
            ]
        }"#
        .to_vec()
    }

    #[test]
    fn the_levels_have_names_and_words_the_best_first() {
        let key = region_levels();
        let named: Vec<(&str, &str)> = key
            .iter()
            .map(|line| (line.name.as_str(), line.words.as_str()))
            .collect();
        assert_eq!(
            named,
            [
                ("Very good", "Strong, high thermals for most of the day"),
                ("Good", "Thermals to leave with"),
                ("Weak", "Weak thermals, maybe to stay up"),
                ("Glides only", "Flyable, nothing to climb in"),
                ("Not flyable", "The forecast rules it out"),
            ]
        );
        assert_eq!(regions_path(), "/v1/regions");
        assert_eq!(
            region_forecast_path(Some("2026-07-12".to_owned())).unwrap(),
            "/v1/regions/forecast?date=2026-07-12"
        );
        assert_eq!(region_forecast_path(None).unwrap(), "/v1/regions/forecast");
        assert!(region_forecast_path(Some("Today".to_owned())).is_err());
    }

    #[test]
    fn a_day_of_the_forecast_by_region_is_read_in_words() {
        let forecast = RegionForecast::read(200, forecast(), 7200, NOON).unwrap();
        let view = forecast.view();
        assert_eq!(view.date, "2026-07-12");
        assert_eq!(view.dates.len(), 2);
        assert_eq!(view.model, "ICON-D2");
        // 10 and 11 UTC are 12:00 and 13:00 by the pilot's clock; today it
        // is noon UTC, so the last hour begun is the second.
        let labels: Vec<&str> = view.hours.iter().map(|hour| hour.label.as_str()).collect();
        assert_eq!(labels, ["12:00", "13:00"]);
        assert_eq!(view.first_hour, 1);
        assert_eq!(view.credits.len(), 2);
        assert!(view.note.is_empty());

        let day = forecast.day();
        assert_eq!(
            (
                day[0].id.as_str(),
                day[0].level,
                day[0].words.as_str(),
                day[0].detail.as_str()
            ),
            ("DE-BY-41", RegionLevel::Good, "Good", "From 3 take-offs")
        );
        assert_eq!(day[1].words, "Not flyable: wind higher up");
        assert_eq!(day[1].detail, "From the model's ground");
        let noon = forecast.hour(1);
        assert_eq!(
            (noon[0].level, noon[0].detail.as_str()),
            (RegionLevel::VeryGood, "")
        );
        assert!(forecast.hour(5).is_empty());
    }

    #[test]
    fn the_regions_are_handed_to_the_map_as_they_are() {
        let body = br#"{
            "type": "FeatureCollection", "version": "eaws-regions 439ec676 2026-11-24",
            "features": [{
                "type": "Feature",
                "properties": {
                    "id": "DE-BY-41",
                    "names": {"de": "Bayrische Voralpen West", "it": "Prealpi Bavaresi Ovest",
                              "en": "Bavarian Prealps West", "fr": "", "sl": "Bavarian Prealps West"},
                    "label": [11.4853, 47.6507]
                },
                "geometry": {"type": "MultiPolygon", "coordinates": [[[
                    [11.4, 47.5], [11.6, 47.5], [11.6, 47.7], [11.4, 47.5]
                ]]]}
            }],
            "attributions": [{"source": "eaws-regions", "provider": "EAWS", "licence": "CC0 1.0",
                "text": "Regions: EAWS (CC0)",
                "url": "https://gitlab.com/eaws/eaws-regions/-/blob/master/LICENSE"}]
        }"#;
        let regions = ForecastRegions::read(200, body.to_vec(), "de".to_owned()).unwrap();
        assert_eq!(regions.version(), "eaws-regions 439ec676 2026-11-24");
        let outlines: serde_json::Value = serde_json::from_str(&regions.outlines()).unwrap();
        assert_eq!(
            outlines["features"][0]["properties"],
            serde_json::json!({"id": "DE-BY-41", "name": "Bayrische Voralpen West"})
        );
        let labels: serde_json::Value = serde_json::from_str(&regions.labels()).unwrap();
        assert_eq!(
            labels["features"][0]["geometry"],
            serde_json::json!({"type": "Point", "coordinates": [11.4853, 47.6507]})
        );
        assert_eq!(regions.credits()[0].text, "Regions: EAWS (CC0)");
        // A language without a name of its own falls back to English.
        let french = ForecastRegions::read(200, body.to_vec(), "fr".to_owned()).unwrap();
        assert_eq!(
            french.name_of("DE-BY-41".to_owned()).as_deref(),
            Some("Bavarian Prealps West")
        );
        assert!(ForecastRegions::read(200, b"{}".to_vec(), "en".to_owned()).is_err());
    }
}
