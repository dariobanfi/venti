use serde::{Deserialize, Serialize};
use venti_domain::{Airspace, BoundingBox, SourceId, Timestamp};

use super::Attribution;

/// The answer of `GET /v1/airspace`.
///
/// Airspace comes in layers, one per data source. The layers are never
/// merged: each is under its own licence, with its own credit and its own
/// date, and a client that stores or draws airspace keeps them apart in
/// the same way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceResponse {
    /// One layer for each source the server holds airspace of, sorted by
    /// source. A source with nothing in the requested box is a layer
    /// without airspaces: it still says how old its set is and where the
    /// set is whole, which is what tells "no airspace here" from "not
    /// held".
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
    /// Where the server's set of this source is whole: inside these boxes
    /// it holds every airspace it could read from what the source
    /// publishes for the countries in [`countries`](Self::countries).
    /// Outside them it holds nothing, or only what happens to reach out
    /// of them.
    ///
    /// This describes the server's set, not the request: it is the same
    /// whatever box was asked for. A client that wants to know whether a
    /// place is covered asks whether the place lies in one of these
    /// boxes, and treats an empty list as "the server does not say",
    /// which is not covered.
    ///
    /// **The boxes take in more than the countries.** A box is the area
    /// the server collects for, and where it reaches across a border into
    /// a country that is not in `countries`, the layer holds nothing of
    /// that country's airspace. The answer cannot say where the border
    /// runs. A client shows the countries beside the airspace.
    #[serde(default)]
    pub covers: Vec<BoundingBox>,
    /// The countries whose airspace the layer holds, as ISO 3166-1 alpha-2
    /// codes: the ones the server fetched from the source. Empty when the
    /// server does not say.
    #[serde(default)]
    pub countries: Vec<String>,
    /// The canonical records, sorted by identifier: every airspace of the
    /// server's set whose bounding box touches the requested box. Empty
    /// when none does.
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
        // An answer that says nothing about where it is whole covers nothing.
        assert!(layer.covers.is_empty() && layer.countries.is_empty());
        let written = serde_json::to_value(&answer).unwrap();
        assert!(written["layers"][0].get("file_date").is_none());
        assert!(written["layers"][0].get("newest_change").is_none());
        assert_eq!(written["layers"][0]["covers"], serde_json::json!([]));
        assert_eq!(
            serde_json::from_value::<AirspaceResponse>(written).unwrap(),
            answer
        );
    }

    #[test]
    fn a_layer_says_where_it_is_whole_in_boxes_with_named_edges() {
        let text = r#"{
            "source": "openaip",
            "fetched_at": "2026-11-19T03:10:00Z",
            "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}],
            "countries": ["DE", "AT"],
            "airspaces": []
        }"#;

        let layer: AirspaceLayer = serde_json::from_str(text).unwrap();

        assert_eq!(
            layer.covers,
            [BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()]
        );
        assert_eq!(layer.countries, ["DE", "AT"]);
        // The sample flight's valley is inside, Hamburg is not.
        let inside = |lat, lon| {
            let place = venti_domain::GeoPoint::new(lat, lon).unwrap();
            layer.covers.iter().any(|area| area.contains(place))
        };
        assert!(inside(47.7, 11.8));
        assert!(!inside(53.6, 9.9));
        let written = serde_json::to_value(&layer).unwrap();
        assert_eq!(
            written["covers"][0],
            serde_json::json!({"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2})
        );
    }
}
