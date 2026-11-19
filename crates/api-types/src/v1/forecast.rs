use serde::{Deserialize, Serialize};
use venti_domain::{ForecastSeries, GeoPoint, SourceId, Timestamp};

use super::Attribution;

/// The answer of `GET /v1/forecast`.
///
/// What the weather models say about one place for today and the next two
/// days, hour by hour. The answer holds magnitudes and no judgement: it
/// does not say whether a day can be flown.
///
/// Forecasts come in layers, one per data source, and within a layer in
/// series, one per weather model. Neither is ever blended: a client that
/// wants one line for the day chooses a series, and says which.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastResponse {
    /// The place the forecast is for: the place that was asked for, moved
    /// to the nearest point of a grid of 0.02°. Two requests for places in
    /// the same cell of that grid get the same answer.
    pub point: GeoPoint,
    /// When the server fetched this answer from its forecast service. An
    /// answer is reused for up to half an hour. Show the pilot the run of
    /// the series they look at; this time says how long ago the server
    /// last made sure there was no newer one.
    pub fetched_at: Timestamp,
    /// One layer for each source that has a series for the place, sorted
    /// by source.
    pub layers: Vec<ForecastLayer>,
    /// The credit for every source that has a layer in the answer, with
    /// what its licence asks a client to show.
    pub attributions: Vec<Attribution>,
}

/// The forecasts of one source for the place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastLayer {
    /// The source. Its credit is in the response's attributions.
    pub source: SourceId,
    /// One series for each weather model that has values for the place,
    /// sorted by model. A model that does not reach the place, or whose
    /// run the source could not tell, is not listed.
    pub series: Vec<ForecastSeries>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_read_back_and_fields_of_a_newer_server_are_ignored() {
        let text = r#"{
            "point": {"lat": 47.7, "lon": 11.8},
            "fetched_at": "2026-11-20T09:40:00Z",
            "layers": [{
                "source": "open-meteo",
                "series": [{
                    "source": "open-meteo",
                    "model": "icon-d2",
                    "run": "2026-11-20T06:00:00Z",
                    "point": {"lat": 47.7, "lon": 11.8},
                    "cell": {"lat": 47.7, "lon": 11.8},
                    "hours": [{
                        "time": "2026-11-20T12:00:00Z",
                        "wind_speed_ms": 2.5,
                        "thermal_index": 7,
                        "winds_aloft": [{"pressure_hpa": 850.0, "wind_speed_ms": 9.4, "shear": 1}]
                    }]
                }]
            }],
            "attributions": []
        }"#;

        let answer: ForecastResponse = serde_json::from_str(text).unwrap();

        let series = &answer.layers[0].series[0];
        assert_eq!(series.model.as_str(), "icon-d2");
        assert_eq!(series.available_at, None);
        assert_eq!(series.cell_elevation_m, None);
        let hour = &series.hours[0];
        assert_eq!(hour.wind_speed_ms, Some(2.5));
        assert_eq!(hour.temperature_c, None);
        assert_eq!(hour.winds_aloft[0].pressure_hpa, Some(850.0));
        assert_eq!(hour.winds_aloft[0].above_ground_m, None);

        // What is not known is left out again, never written as null.
        let written = serde_json::to_string(&answer).unwrap();
        assert!(!written.contains("null"), "{written}");
        assert_eq!(
            serde_json::from_str::<ForecastResponse>(&written).unwrap(),
            answer
        );
    }
}
