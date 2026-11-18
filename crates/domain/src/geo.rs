use serde::{Deserialize, Serialize};

/// Why a coordinate or area was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GeoError {
    /// Latitude outside -90..=90 or not a finite number.
    #[error("latitude {0} is outside -90..=90")]
    Latitude(f64),
    /// Longitude outside -180..=180 or not a finite number.
    #[error("longitude {0} is outside -180..=180")]
    Longitude(f64),
    /// The southern edge of a box lies north of its northern edge, or west lies east of east.
    #[error("bounding box edges are in the wrong order")]
    InvertedBox,
}

/// A position on the WGS84 ellipsoid, in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPoint", into = "RawPoint")]
pub struct GeoPoint {
    lat: f64,
    lon: f64,
}

#[derive(Serialize, Deserialize)]
struct RawPoint {
    lat: f64,
    lon: f64,
}

impl GeoPoint {
    /// Creates a point, rejecting coordinates that are out of range or not finite.
    pub fn new(lat: f64, lon: f64) -> Result<Self, GeoError> {
        if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
            return Err(GeoError::Latitude(lat));
        }
        if !lon.is_finite() || !(-180.0..=180.0).contains(&lon) {
            return Err(GeoError::Longitude(lon));
        }
        Ok(Self { lat, lon })
    }

    /// Latitude in degrees, positive north.
    pub fn lat(&self) -> f64 {
        self.lat
    }

    /// Longitude in degrees, positive east.
    pub fn lon(&self) -> f64 {
        self.lon
    }
}

impl TryFrom<RawPoint> for GeoPoint {
    type Error = GeoError;
    fn try_from(raw: RawPoint) -> Result<Self, Self::Error> {
        Self::new(raw.lat, raw.lon)
    }
}

impl From<GeoPoint> for RawPoint {
    fn from(p: GeoPoint) -> Self {
        Self {
            lat: p.lat,
            lon: p.lon,
        }
    }
}

/// A rectangle in latitude and longitude.
///
/// It does not cross the antimeridian; no region this project plans needs that.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawBox", into = "RawBox")]
pub struct BoundingBox {
    south: f64,
    west: f64,
    north: f64,
    east: f64,
}

#[derive(Serialize, Deserialize)]
struct RawBox {
    south: f64,
    west: f64,
    north: f64,
    east: f64,
}

impl BoundingBox {
    /// Creates a box from its four edges in degrees.
    pub fn new(south: f64, west: f64, north: f64, east: f64) -> Result<Self, GeoError> {
        let sw = GeoPoint::new(south, west)?;
        let ne = GeoPoint::new(north, east)?;
        if sw.lat() > ne.lat() || sw.lon() > ne.lon() {
            return Err(GeoError::InvertedBox);
        }
        Ok(Self {
            south,
            west,
            north,
            east,
        })
    }

    /// Whether the point lies inside the box, edges included.
    pub fn contains(&self, point: GeoPoint) -> bool {
        (self.south..=self.north).contains(&point.lat())
            && (self.west..=self.east).contains(&point.lon())
    }
}

impl TryFrom<RawBox> for BoundingBox {
    type Error = GeoError;
    fn try_from(raw: RawBox) -> Result<Self, Self::Error> {
        Self::new(raw.south, raw.west, raw.north, raw.east)
    }
}

impl From<BoundingBox> for RawBox {
    fn from(b: BoundingBox) -> Self {
        Self {
            south: b.south,
            west: b.west,
            north: b.north,
            east: b.east,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_rejects_out_of_range_and_non_finite() {
        assert!(GeoPoint::new(47.42, 10.98).is_ok());
        assert!(GeoPoint::new(91.0, 0.0).is_err());
        assert!(GeoPoint::new(0.0, 181.0).is_err());
        assert!(GeoPoint::new(f64::NAN, 0.0).is_err());
    }

    #[test]
    fn box_contains_points_on_its_edges() {
        let alps = BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap();
        assert!(alps.contains(GeoPoint::new(47.42, 10.98).unwrap()));
        assert!(alps.contains(GeoPoint::new(46.3, 9.4).unwrap()));
        assert!(!alps.contains(GeoPoint::new(52.9, 12.8).unwrap()));
    }

    #[test]
    fn box_rejects_inverted_edges() {
        assert_eq!(
            BoundingBox::new(48.0, 9.0, 46.0, 17.0),
            Err(GeoError::InvertedBox)
        );
    }
}
