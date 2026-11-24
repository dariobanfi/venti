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
//!
//! A forecast has two kinds of height beside those above mean sea level: a
//! height above the model's own, smoothed ground, and a surface of equal
//! air pressure. Each field says which it is.

mod airspace;
mod area;
mod cable;
mod forecast;
mod geo;
mod ids;
mod observation;
mod radar;
mod site;
mod station;

pub use airspace::{
    Activity, Airspace, AirspaceClass, AirspaceError, AirspaceType, Circle, DayTime, Frequency,
    HeightUnit, MAX_HOLES, MAX_RING_POINTS, MAX_SPAN_DEG, METRES_PER_FOOT, OperatingHours, Polygon,
    Validity, VerticalLimit, VerticalReference,
};
pub use area::{Area, AreaError};
pub use cable::{Cable, CableKind};
pub use forecast::{ForecastHour, ForecastSeries, WindAloft};
pub use geo::{BoundingBox, GeoError, GeoPoint};
pub use ids::{AirspaceId, IdError, ModelId, SiteId, SourceId, StationId};
pub use observation::Observation;
pub use radar::{
    RADAR_DRY, RADAR_MAX_SIDE, RADAR_STEP_DEG, RADAR_UNSEEN, RAIN_STEPS_MM_H, RadarFrame,
    RadarGrid, rain_step,
};
pub use site::{Site, SiteFault, SiteOutline, SiteOutlineError, SiteUse, WindSector};
pub use station::Station;

/// A point in time, always UTC.
pub type Timestamp = chrono::DateTime<chrono::Utc>;
