//! An area drawn on a map: a simple polygon in longitude and latitude.

use thiserror::Error;

use crate::GeoPoint;

/// An area drawn on the map: a simple polygon of three to
/// [`Area::MAX_CORNERS`] corners, in the order they were drawn. Its edges
/// cross nowhere and it is not closed by repeating the first corner.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    corners: Vec<GeoPoint>,
}

/// Why corners are no area.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AreaError {
    /// Fewer than three or more than [`Area::MAX_CORNERS`] corners.
    #[error("{0} corners, and an area has three to {max}", max = Area::MAX_CORNERS)]
    Corners(usize),
    /// Two edges cross, or it has no surface.
    #[error("no simple polygon: its edges cross or it has no surface")]
    NotSimple,
    /// It reaches across more than half the globe.
    #[error("it must lie within 180 degrees of longitude")]
    TooWide,
}

impl Area {
    /// The most corners an area has.
    pub const MAX_CORNERS: usize = 50;

    /// An area of these corners, refused unless it is a simple polygon
    /// within the bounds. A first corner repeated at the end is dropped.
    pub fn new(mut corners: Vec<GeoPoint>) -> Result<Self, AreaError> {
        if corners.len() > 3 && corners.first() == corners.last() {
            corners.pop();
        }
        if !(3..=Self::MAX_CORNERS).contains(&corners.len()) {
            return Err(AreaError::Corners(corners.len()));
        }
        let (west, east) = corners
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(w, e), corner| {
                (w.min(corner.lon()), e.max(corner.lon()))
            });
        if east - west > 180.0 {
            return Err(AreaError::TooWide);
        }
        let area = Self { corners };
        if !area.is_simple() {
            return Err(AreaError::NotSimple);
        }
        Ok(area)
    }

    /// The corners, in the order they were drawn.
    pub fn corners(&self) -> &[GeoPoint] {
        &self.corners
    }

    /// The box around it: west, south, east, north.
    pub fn bounds(&self) -> [f64; 4] {
        self.corners.iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |[w, s, e, n], c| {
                [
                    w.min(c.lon()),
                    s.min(c.lat()),
                    e.max(c.lon()),
                    n.max(c.lat()),
                ]
            },
        )
    }

    /// The edges, each from a corner to the next, the last back to the
    /// first, as longitude and latitude.
    fn edges(&self) -> impl Iterator<Item = ([f64; 2], [f64; 2])> + '_ {
        let n = self.corners.len();
        (0..n).map(move |i| {
            let (a, b) = (self.corners[i], self.corners[(i + 1) % n]);
            ([a.lon(), a.lat()], [b.lon(), b.lat()])
        })
    }

    /// Whether no two edges meet but neighbours at their shared corner,
    /// and the polygon has a surface.
    fn is_simple(&self) -> bool {
        let edges: Vec<_> = self.edges().collect();
        let n = edges.len();
        let twice_area: f64 = edges.iter().map(|(a, b)| a[0] * b[1] - b[0] * a[1]).sum();
        if twice_area.abs() < 1e-12 {
            return false;
        }
        for i in 0..n {
            for j in i + 1..n {
                let neighbours = j == i + 1 || (i == 0 && j == n - 1);
                let (a, b) = edges[i];
                let (c, d) = edges[j];
                if neighbours {
                    // Neighbours share a corner; they may not fold back
                    // onto each other.
                    let shared_turn = cross(a, b, d);
                    if j == i + 1 && shared_turn == 0.0 && dot(a, b, d) > 0.0 {
                        return false;
                    }
                    continue;
                }
                if segments_meet(a, b, c, d) {
                    return false;
                }
            }
        }
        true
    }

    /// Whether a place lies inside the area or on its edge, on the plane of
    /// longitude and latitude, as PostGIS's `ST_Intersects` decides it for
    /// a point and a polygon in degrees.
    pub fn holds(&self, place: GeoPoint) -> bool {
        let p = [place.lon(), place.lat()];
        let mut inside = false;
        for (a, b) in self.edges() {
            if on_segment(a, b, p) {
                return true;
            }
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if p[0] < x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// The area as well-known text, longitude first, closed: what PostGIS
    /// reads.
    pub fn wkt(&self) -> String {
        let mut ring: Vec<String> = self
            .corners
            .iter()
            .map(|c| format!("{} {}", c.lon(), c.lat()))
            .collect();
        ring.push(ring[0].clone());
        format!("POLYGON(({}))", ring.join(","))
    }
}

/// Which side of the line from `a` to `b` the point `c` lies on: positive
/// left, negative right, zero on it.
fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Whether `c`, seen from `b`, lies back towards `a`.
fn dot(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (a[0] - b[0]) * (c[0] - b[0]) + (a[1] - b[1]) * (c[1] - b[1])
}

fn on_segment(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    cross(a, b, p) == 0.0
        && p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

fn segments_meet(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    on_segment(c, d, a) || on_segment(c, d, b) || on_segment(a, b, c) || on_segment(a, b, d)
}
