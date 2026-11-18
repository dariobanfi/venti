use serde::{Deserialize, Serialize};

use crate::{GeoPoint, StationId};

/// A fixed place where weather is measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Station {
    /// Identifier, scoped to the source that reports the station.
    pub id: StationId,
    /// Name as the provider publishes it.
    pub name: String,
    /// Where the station stands.
    pub position: GeoPoint,
    /// Height of the station above mean sea level in metres, if published.
    pub elevation_m: Option<f64>,
}
