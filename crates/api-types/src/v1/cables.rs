use serde::{Deserialize, Serialize};
use venti_domain::{BoundingBox, Cable, SourceId, Timestamp};

use super::{Attribution, format_bbox};

/// Path of the cables inside a box.
pub const CABLES_PATH: &str = "/v1/cables";

/// The most cables one response carries. A box that holds more is refused
/// with [`ErrorCode::BoxTooLarge`](super::ErrorCode::BoxTooLarge) and not
/// cut short, as for airspace: a cut answer would pass for all the cables
/// of the box, and the cockpit would not watch for the ones left out.
pub const MAX_CABLES: usize = 5000;

/// A request for the cables that touch a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CablesRequest {
    /// The box to list.
    pub bbox: BoundingBox,
}

impl CablesRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        format!("{CABLES_PATH}?bbox={}", format_bbox(&self.bbox))
    }
}

/// The answer of `GET /v1/cables`.
///
/// Cables come in layers, one per data source, never merged with another
/// source's records, under their own licence and credit.
///
/// **No cable in an answer is no statement that there is none.** The
/// sources are incomplete: OpenStreetMap has many power lines and few of
/// the material ropeways and low lines that a pilot sees last.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CablesResponse {
    /// One layer for each source the server holds cables of, sorted by
    /// source; a source with nothing in the box is a layer without cables.
    pub layers: Vec<CableLayer>,
    /// The credit for every source that has a layer in the answer.
    pub attributions: Vec<Attribution>,
}

/// The cables of one source inside the requested box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CableLayer {
    /// The source. Its credit is in the response's attributions.
    pub source: SourceId,
    /// When the server fetched the set this layer is a part of.
    pub fetched_at: Timestamp,
    /// The date the source's data is of, by its own word.
    pub as_of: Timestamp,
    /// Where the server's set of this source is whole. Outside these boxes
    /// it holds nothing; an empty list is "the server does not say".
    #[serde(default)]
    pub covers: Vec<BoundingBox>,
    /// Every cable of the set whose line's box touches the requested box,
    /// sorted by key. A height is given only where the source maps one.
    pub cables: Vec<Cable>,
}
