use serde::{Deserialize, Serialize};
use venti_domain::{Airspace, SourceId, Timestamp};

use super::Attribution;

/// The answer of `GET /v1/airspace`.
///
/// Airspace comes in layers, one per data source. The layers are never
/// merged: each is under its own licence, with its own credit and its own
/// date, and a client that stores or draws airspace keeps them apart in
/// the same way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceResponse {
    /// One layer for each source that has airspace touching the requested
    /// box, sorted by source.
    pub layers: Vec<AirspaceLayer>,
    /// The credit for every source that has a layer in the answer, with
    /// what its licence asks a client to show.
    pub attributions: Vec<Attribution>,
}

/// The airspace of one source inside the requested box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceLayer {
    /// The source. Its credit is in the response's attributions, and every
    /// airspace of the layer names it in its identifier.
    pub source: SourceId,
    /// When the server fetched the set this layer is a part of. Show the
    /// pilot how old that is, measured against the server's clock.
    pub fetched_at: Timestamp,
    /// The date of the oldest file the source's set was read from: when
    /// the source wrote it, by its own word. Where a source does not say,
    /// it is the time of the fetch. Each airspace carries the date of its
    /// own file in `validity.file_date`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_date: Option<Timestamp>,
    /// When the source last changed any airspace of its set, if it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest_change: Option<Timestamp>,
    /// The canonical records, sorted by identifier: every airspace of the
    /// source whose bounding box touches the requested box.
    pub airspaces: Vec<Airspace>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_is_read_back_and_dates_a_source_does_not_give_are_left_out() {
        let text = r#"{
            "layers": [{
                "source": "openaip",
                "fetched_at": "2026-11-19T03:10:00Z",
                "airspaces": [{
                    "id": "openaip:677d59be13e31af480aca6d9",
                    "name": "CTR INNSBRUCK",
                    "class": "D",
                    "airspace_type": "ctr",
                    "country": "AT",
                    "lower": {"reference": "ground", "metres": 0.0},
                    "upper": {"reference": "mean-sea-level", "value": 7000.0, "unit": "feet", "metres": 2133.6},
                    "geometry": {"exterior": [[11.2, 47.2], [11.5, 47.2], [11.5, 47.3], [11.2, 47.2]]},
                    "activity": {"by_notam": false, "on_request": false, "on_demand": false, "special_agreement": false},
                    "validity": {"file_date": "2026-11-19T01:12:30Z"},
                    "added_by_a_newer_server": true
                }]
            }],
            "attributions": []
        }"#;

        let answer: AirspaceResponse = serde_json::from_str(text).unwrap();

        let layer = &answer.layers[0];
        assert_eq!(layer.source.as_str(), "openaip");
        assert_eq!(layer.file_date, None);
        assert_eq!(layer.airspaces[0].id.source(), &layer.source);
        let written = serde_json::to_value(&answer).unwrap();
        assert!(written["layers"][0].get("file_date").is_none());
        assert!(written["layers"][0].get("newest_change").is_none());
        assert_eq!(
            serde_json::from_value::<AirspaceResponse>(written).unwrap(),
            answer
        );
    }
}
