use serde::{Deserialize, Serialize};
use venti_domain::{BoundingBox, Site, SourceId, Timestamp};

use super::Attribution;

/// The answer of `GET /v1/sites`.
///
/// Flying sites come in layers, one per data source, as airspace does. The
/// layers are never merged: each is under its own licence, with its own
/// credit and its own date, and a client that stores or draws sites keeps
/// them apart in the same way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SitesResponse {
    /// One layer for each source the server holds sites of, sorted by
    /// source. A source with nothing in the requested box is a layer
    /// without sites: it still says how old its set is and where the set
    /// is whole.
    pub layers: Vec<SiteLayer>,
    /// The credit for every source that has a layer in the answer, with
    /// what its licence asks a client to show.
    pub attributions: Vec<Attribution>,
}

/// The flying sites of one source inside the requested box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteLayer {
    /// The source. Its credit is in the response's attributions, and every
    /// site of the layer names it in its identifier.
    pub source: SourceId,
    /// When the server fetched the set this layer is a part of.
    pub fetched_at: Timestamp,
    /// The date the source's data is of, by the source's own word: for
    /// OpenStreetMap, the moment the copy that answered was taken. Show
    /// the pilot how old that is, measured against the server's clock.
    pub as_of: Timestamp,
    /// Where the server's set of this source is whole: inside these boxes
    /// it holds every site the source has. Outside them it holds nothing.
    ///
    /// This describes the server's set, not the request: it is the same
    /// whatever box was asked for. An empty list means the server does
    /// not say, which is not covered.
    ///
    /// **Whole is not complete.** The set is everything the source has,
    /// and a source that anyone can edit has what somebody entered. Where
    /// a layer shows no site there may well be one.
    #[serde(default)]
    pub covers: Vec<BoundingBox>,
    /// The canonical records, sorted by identifier: every site of the
    /// server's set inside the requested box, edges included. Empty when
    /// there is none.
    pub sites: Vec<Site>,
}

#[cfg(test)]
mod tests {
    use venti_domain::SiteUse;

    use super::*;

    #[test]
    fn a_layer_is_read_back_and_what_a_site_does_not_say_is_left_out() {
        let text = r#"{
            "layers": [{
                "source": "osm-free-flying",
                "fetched_at": "2026-11-19T18:10:00Z",
                "as_of": "2026-11-18T21:21:02Z",
                "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}],
                "sites": [{
                    "id": "osm-free-flying:node/5964676196",
                    "position": {"lat": 47.6652, "lon": 11.7954},
                    "uses": ["takeoff"],
                    "url": "https://www.openstreetmap.org/node/5964676196",
                    "added_by_a_newer_server": true
                }]
            }],
            "attributions": []
        }"#;

        let answer: SitesResponse = serde_json::from_str(text).unwrap();

        let layer = &answer.layers[0];
        assert_eq!(layer.source.as_str(), "osm-free-flying");
        let site = &layer.sites[0];
        assert_eq!(site.id.source(), &layer.source);
        assert_eq!(site.uses, [SiteUse::Takeoff]);
        assert_eq!(site.name, None);
        assert!(site.wind_directions.is_empty());
        assert!(!site.discouraged);
        let written = serde_json::to_value(&answer).unwrap();
        let site = &written["layers"][0]["sites"][0];
        assert!(site.get("name").is_none());
        assert!(site.get("wind_directions").is_none());
        assert!(site.get("discouraged").is_none());
        assert_eq!(
            serde_json::from_value::<SitesResponse>(written).unwrap(),
            answer
        );

        // An answer that says nothing about where it is whole covers nothing.
        let bare = r#"{"source": "osm-free-flying", "fetched_at": "2026-11-19T18:10:00Z",
                       "as_of": "2026-11-18T21:21:02Z", "sites": []}"#;
        assert!(
            serde_json::from_str::<SiteLayer>(bare)
                .unwrap()
                .covers
                .is_empty()
        );
    }
}
