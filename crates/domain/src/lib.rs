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
//!
//! Airspace limits are the one exception to the height rule. A limit keeps
//! the value, unit and reference it was published with (`FL 95`,
//! `1000 ft AGL`), because turning it into metres above sea level needs the
//! air pressure or the terrain, which a record does not have.

mod airspace;
mod geo;
mod ids;
mod observation;
mod station;

pub use airspace::{
    Activity, Airspace, AirspaceClass, AirspaceError, AirspaceType, Circle, DayTime, Frequency,
    HeightUnit, MAX_HOLES, MAX_RING_POINTS, MAX_SPAN_DEG, METRES_PER_FOOT, OperatingHours, Polygon,
    Validity, VerticalLimit, VerticalReference,
};
pub use geo::{BoundingBox, GeoError, GeoPoint};
pub use ids::{AirspaceId, IdError, SourceId, StationId};
pub use observation::Observation;
pub use station::Station;

/// A point in time, always UTC.
pub type Timestamp = chrono::DateTime<chrono::Utc>;
