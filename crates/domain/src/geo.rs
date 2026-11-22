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

    /// Great-circle distance to another point in metres, on the FAI sphere.
    ///
    /// The FAI sphere has a radius of 6371 km. Free-flight distances are
    /// conventionally measured on it or on the WGS84 ellipsoid; the two differ
    /// by at most about half a percent. Scoring that must match a league's
    /// rules chooses its own earth model.
    pub fn distance_m(&self, other: GeoPoint) -> f64 {
        const FAI_SPHERE_RADIUS_M: f64 = 6_371_000.0;
        let (lat1, lat2) = (self.lat.to_radians(), other.lat.to_radians());
        let dlat = lat2 - lat1;
        let dlon = (other.lon - self.lon).to_radians();
        let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        2.0 * FAI_SPHERE_RADIUS_M * a.sqrt().min(1.0).asin()
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

    /// Southern edge, degrees of latitude.
    pub fn south(&self) -> f64 {
        self.south
    }

    /// Western edge, degrees of longitude.
    pub fn west(&self) -> f64 {
        self.west
    }

    /// Northern edge, degrees of latitude.
    pub fn north(&self) -> f64 {
        self.north
    }

    /// Eastern edge, degrees of longitude.
    pub fn east(&self) -> f64 {
        self.east
    }

    /// Whether the point lies inside the box, edges included.
    pub fn contains(&self, point: GeoPoint) -> bool {
        (self.south..=self.north).contains(&point.lat())
            && (self.west..=self.east).contains(&point.lon())
    }

    /// Whether all of `other` lies inside the box, edges included.
    pub fn encloses(&self, other: &BoundingBox) -> bool {
        self.south <= other.south
            && self.west <= other.west
            && self.north >= other.north
            && self.east >= other.east
    }

    /// The smallest box that contains both this box and `other`.
    pub fn cover(&self, other: &BoundingBox) -> BoundingBox {
        BoundingBox {
            south: self.south.min(other.south),
            west: self.west.min(other.west),
            north: self.north.max(other.north),
            east: self.east.max(other.east),
        }
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
    fn distance_matches_known_values() {
        // One degree of latitude on a 6371 km sphere is 111.195 km.
        let a = GeoPoint::new(47.0, 11.0).unwrap();
        let b = GeoPoint::new(48.0, 11.0).unwrap();
        assert!((a.distance_m(b) - 111_194.9).abs() < 1.0);
        assert_eq!(a.distance_m(a), 0.0);
        // Munich to Innsbruck is a little under 100 km.
        let munich = GeoPoint::new(48.137, 11.575).unwrap();
        let innsbruck = GeoPoint::new(47.269, 11.404).unwrap();
        let km = munich.distance_m(innsbruck) / 1000.0;
        assert!((97.0..98.5).contains(&km), "{km}");
    }

    #[test]
    fn box_contains_points_on_its_edges() {
        let alps = BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap();
        assert!(alps.contains(GeoPoint::new(47.42, 10.98).unwrap()));
        assert!(alps.contains(GeoPoint::new(46.3, 9.4).unwrap()));
        assert!(!alps.contains(GeoPoint::new(52.9, 12.8).unwrap()));
    }

    #[test]
    fn cover_contains_both_boxes() {
        let alps = BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap();
        let north = BoundingBox::new(52.0, 8.0, 54.0, 12.0).unwrap();
        let both = alps.cover(&north);
        assert_eq!(
            (both.south(), both.west(), both.north(), both.east()),
            (46.3, 8.0, 54.0, 17.2)
        );
        assert_eq!(alps.cover(&alps), alps);
        assert!(both.encloses(&alps) && both.encloses(&north) && alps.encloses(&alps));
        assert!(!alps.encloses(&both));
    }

    #[test]
    fn box_rejects_inverted_edges() {
        assert_eq!(
            BoundingBox::new(48.0, 9.0, 46.0, 17.0),
            Err(GeoError::InvertedBox)
        );
    }
}
