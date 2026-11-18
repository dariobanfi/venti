//! Canonical data model.
//!
//! Every adapter converts provider data into these types before anything else
//! touches it. Nothing outside a `sources-*` crate knows a provider's own format.
//!
//! Conventions used by every type in this crate:
//!
//! - timestamps are UTC;
//! - coordinates are WGS84 degrees;
//! - heights are metres above mean sea level;
//! - wind speed is metres per second, and wind direction is the direction the
//!   wind comes *from*, in degrees clockwise from true north;
//! - temperature is degrees Celsius, pressure is hectopascal, humidity is percent.

mod geo;
mod ids;
mod observation;
mod station;

pub use geo::{BoundingBox, GeoError, GeoPoint};
pub use ids::{IdError, SourceId, StationId};
pub use observation::Observation;
pub use station::Station;

/// A point in time, always UTC.
pub type Timestamp = chrono::DateTime<chrono::Utc>;
