use serde::{Deserialize, Serialize};
use venti_domain::{ForecastSeries, GeoPoint, ModelId, SourceId, Timestamp};

use super::Attribution;

/// The answer of `GET /v1/forecast`.
///
/// What the weather models say about one place, hour by hour, for the
/// hours of today and the days after it that the newest run of each model
/// computed: two days after today, and a third of a model whose runs
/// reach it. The answer holds magnitudes and no judgement: it does not say
/// whether a day can be flown.
///
/// Forecasts come in layers, one per data source, and within a layer in
/// series, one per weather model. Neither is ever blended: a client that
/// wants one line for the day chooses a series, and says which.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastResponse {
    /// The place the forecast is for: the place that was asked for, moved
    /// to the nearest point of a grid of 0.02°. Two requests for places in
    /// the same cell of that grid get the same answer. A server's
    /// development mode has recorded forecasts for a few places and answers
    /// with the nearest of them; this field says which, so a client shows
    /// the forecast as being for this place and not for the one it asked.
    pub point: GeoPoint,
    /// When the server fetched this answer from its forecast service. An
    /// answer is reused for up to half an hour. Show the pilot the run of
    /// the series they look at; this time says how long ago the server
    /// last made sure there was no newer one.
    pub fetched_at: Timestamp,
    /// One layer for each source that has a series for the place, sorted
    /// by source.
    pub layers: Vec<ForecastLayer>,
    /// The strongest wind and the strongest gust the models have near the
    /// place, hour by hour. Left out by a server that makes no thermal
    /// map, while its map is not made yet, and for a place with no cell
    /// of the map near it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nearby: Option<ForecastNearby>,
    /// The credit for every source that has a layer in the answer, with
    /// what its licence asks a client to show.
    pub attributions: Vec<Attribution>,
}

/// The wind around the place: for each daylight hour the server's thermal
/// map keeps, the strongest mean wind and the strongest gust among the
/// cells of that map near the place.
///
/// Magnitudes, like the rest of the answer: whether that is much against
/// what the place itself has is the reader's rule, not the server's. Read
/// from what the map already holds, so it costs no request to the
/// forecast service, and it is as coarse as the map: a cell of the map
/// stands for one cell of the model at its centre, not for every cell of
/// the model inside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastNearby {
    /// The source whose models these are: one that has a layer in the
    /// answer.
    pub source: SourceId,
    /// How far from the place a cell of the map counts as near, km.
    pub radius_km: f64,
    /// How many cells of the map lie within that distance.
    pub cells: u32,
    /// The hours, oldest first, from the first hour any series of the
    /// answer has. Daylight hours only: the map keeps no others.
    pub hours: Vec<NearbyHour>,
}

/// One hour of the wind around a place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NearbyHour {
    /// The moment the values are for, in UTC.
    pub time: Timestamp,
    /// The model the map has the hour's day from. It need not be the model
    /// a client reads the place's own hour from.
    pub model: ModelId,
    /// When the newest run of that model began that the map has an hour
    /// of the day from.
    pub run: Timestamp,
    /// The strongest mean wind 10 m above the model's ground among the
    /// cells near the place. Left out when no cell has a wind in the hour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind: Option<NearbyStrongest>,
    /// The strongest gust there in the hour, likewise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gust: Option<NearbyStrongest>,
}

/// The strongest wind or gust near a place, and where it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NearbyStrongest {
    /// Its speed, whole km/h, as the map keeps it.
    pub kmh: u16,
    /// The centre of the map's cell that has it: of equally strong ones
    /// the nearest to the place.
    pub at: GeoPoint,
    /// The direction the mean wind of that cell comes from, degrees to
    /// the nearest ten. Left out when the model gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_deg: Option<u16>,
    /// The height of the model's ground in that cell, metres above mean
    /// sea level. A cell on a ridge has more wind than one in a valley on
    /// any day: show the height with the wind. Left out for a cell of
    /// water.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_m: Option<i32>,
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
        // An answer of a server that has no thermal map has no wind near
        // by, and says nothing of it.
        assert_eq!(answer.nearby, None);

        // What is not known is left out again, never written as null.
        let written = serde_json::to_string(&answer).unwrap();
        assert!(!written.contains("null"), "{written}");
        assert_eq!(
            serde_json::from_str::<ForecastResponse>(&written).unwrap(),
            answer
        );
    }
}
