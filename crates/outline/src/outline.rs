//! An outline on the map, and where a tile and each of its pixels lie
//! with respect to it.
//!
//! An outline is one or more rings of positions: a polygon's boundary, the
//! boundaries of its holes, further polygons. A point is inside when a line
//! from it to the east crosses the rings an odd number of times, so a hole
//! is outside and a second polygon inside without anything being said
//! about which ring is which.
//!
//! Around the outline lies a band as wide as the margin on each side: what
//! is within the margin of a ring, measured on the ground. A tile that the
//! band reaches is on the edge; any other is wholly inside or wholly
//! outside.
//!
//! **The index.** A country's outline has many points (Austria's some
//! 20,000, France's some 112,000), and most tiles are kilometres from it. So the outline is read once into a grid of a few
//! hundred cells on a side over its box. A cell the band reaches lists the
//! segments of the rings that reach it; any other cell knows whether it is
//! inside or outside. A tile far from the border looks at the cells under
//! it and is answered from them. Each row of the grid also lists the
//! segments that cross it from north to south, which is all that a line to
//! the east from a point in that row can cross.

use std::cmp::Ordering;

use crate::mercator::{self, Point, Rect};

/// Why a text is not an outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineError(pub String);

impl std::fmt::Display for OutlineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OutlineError {}

/// Where a tile lies with respect to an outline and its band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Inside the outline and out of the band.
    Inside,
    /// Outside the outline and out of the band.
    Outside,
    /// The band reaches it.
    Edge,
}

/// Where one pixel lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Inside the outline shrunk by the margin.
    Inside,
    /// Within the margin of the outline, on either side.
    Band,
    /// Outside the outline grown by the margin.
    Outside,
}

/// One straight piece of a ring, in world fractions.
#[derive(Debug, Clone, Copy)]
struct Segment {
    a: Point,
    b: Point,
}

impl Segment {
    fn v_span(&self) -> (f64, f64) {
        (self.a.v.min(self.b.v), self.a.v.max(self.b.v))
    }

    /// Where the segment crosses the line of `v`, if it does. A segment
    /// counts at its northern end and not at its southern, so a line
    /// through a corner of two segments is crossed once.
    fn crossing(&self, v: f64) -> Option<f64> {
        if (self.a.v > v) == (self.b.v > v) {
            return None;
        }
        let t = (v - self.a.v) / (self.b.v - self.a.v);
        Some(self.a.u + t * (self.b.u - self.a.u))
    }

    /// Whether the segment has a point inside `rect`.
    fn meets(&self, rect: Rect) -> bool {
        // Liang and Barsky: clip the segment's parameter to each side.
        let (du, dv) = (self.b.u - self.a.u, self.b.v - self.a.v);
        let mut low = 0.0_f64;
        let mut high = 1.0_f64;
        for (p, q) in [
            (-du, self.a.u - rect.u0),
            (du, rect.u1 - self.a.u),
            (-dv, self.a.v - rect.v0),
            (dv, rect.v1 - self.a.v),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return false;
                }
            } else {
                let t = q / p;
                if p < 0.0 {
                    low = low.max(t);
                } else {
                    high = high.min(t);
                }
                if low > high {
                    return false;
                }
            }
        }
        true
    }

    /// The stretch of the line of `v` that lies within `r` of the segment,
    /// as its western and eastern end, if any of it does.
    ///
    /// The points within `r` of a segment are two discs, one at each end,
    /// and the rectangle between them. All three are convex, and so is
    /// what they make together; its stretch of a line runs from the
    /// westernmost to the easternmost point where one of the three meets
    /// the line.
    fn reach(&self, v: f64, r: f64) -> Option<(f64, f64)> {
        let mut west = f64::INFINITY;
        let mut east = f64::NEG_INFINITY;
        let mut take = |u: f64| {
            west = west.min(u);
            east = east.max(u);
        };
        for end in [self.a, self.b] {
            let dv = v - end.v;
            if dv.abs() <= r {
                let du = (r * r - dv * dv).sqrt();
                take(end.u - du);
                take(end.u + du);
            }
        }
        let (du, dv) = (self.b.u - self.a.u, self.b.v - self.a.v);
        let length = du.hypot(dv);
        if length > 0.0 {
            let (nu, nv) = (-dv / length * r, du / length * r);
            let corners = [
                Point {
                    u: self.a.u + nu,
                    v: self.a.v + nv,
                },
                Point {
                    u: self.b.u + nu,
                    v: self.b.v + nv,
                },
                Point {
                    u: self.b.u - nu,
                    v: self.b.v - nv,
                },
                Point {
                    u: self.a.u - nu,
                    v: self.a.v - nv,
                },
            ];
            for i in 0..4 {
                let (p, q) = (corners[i], corners[(i + 1) % 4]);
                if p.v == q.v {
                    if p.v == v {
                        take(p.u);
                        take(q.u);
                    }
                } else if (p.v.min(q.v)..=p.v.max(q.v)).contains(&v) {
                    take(p.u + (v - p.v) / (q.v - p.v) * (q.u - p.u));
                }
            }
        }
        (west <= east).then_some((west, east))
    }
}

/// What a cell of the index knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Inside,
    Outside,
    /// The band reaches it: the segments that reach it are
    /// `cell_segments[first..first + count]`.
    Edge {
        first: u32,
        count: u32,
    },
}

/// An outline with its index, ready to be asked about tiles.
#[derive(Debug, Clone)]
pub struct Outline {
    segments: Vec<Segment>,
    margin_m: f64,
    /// The margin in world fractions where the outline is furthest from
    /// the equator: no less than it is anywhere on the outline.
    margin_units: f64,
    /// The box of the grid: the outline's box, grown by the margin and one
    /// cell.
    grid: Rect,
    cell_side: f64,
    columns: usize,
    rows: usize,
    cells: Vec<Cell>,
    cell_segments: Vec<u32>,
    /// For each row, the segments whose span from north to south reaches
    /// into it.
    row_segments: Vec<Vec<u32>>,
}

/// How many cells the longer side of the grid has.
const GRID_CELLS: usize = 256;

impl Outline {
    /// An outline of the given rings, each a list of longitude and latitude
    /// in degrees, with a band of `margin_m` metres on each side.
    ///
    /// A ring needs three different positions. It is closed whether or not
    /// its last position repeats its first.
    pub fn new(rings: &[Vec<(f64, f64)>], margin_m: f64) -> Result<Self, OutlineError> {
        if !(margin_m.is_finite() && margin_m > 0.0) {
            return Err(OutlineError(format!(
                "a margin of {margin_m} m is no margin"
            )));
        }
        if rings.is_empty() {
            return Err(OutlineError("an outline needs a ring".to_owned()));
        }
        let mut segments = Vec::new();
        let mut furthest_lat = 0.0_f64;
        for (number, ring) in rings.iter().enumerate() {
            let mut ring: Vec<(f64, f64)> = ring.clone();
            if ring.len() > 1 && ring.first() == ring.last() {
                ring.pop();
            }
            if ring.len() < 3 {
                return Err(OutlineError(format!(
                    "ring {number} has fewer than three positions"
                )));
            }
            for &(lon, lat) in &ring {
                if !((-180.0..=180.0).contains(&lon) && (-85.0..=85.0).contains(&lat)) {
                    return Err(OutlineError(format!(
                        "ring {number} has a position off the map: {lon}, {lat}"
                    )));
                }
                furthest_lat = furthest_lat.max(lat.abs());
            }
            let points: Vec<Point> = ring
                .iter()
                .map(|&(lon, lat)| mercator::project(lon, lat))
                .collect();
            for (i, &a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                segments.push(Segment { a, b });
            }
        }

        // A little more than the margin at the furthest latitude, so that
        // a tile's own margin, worked out at its middle, never exceeds it.
        let margin_units = margin_m * mercator::units_per_metre((furthest_lat + 0.5).min(85.0));
        let mut bounds = Rect {
            u0: f64::INFINITY,
            v0: f64::INFINITY,
            u1: f64::NEG_INFINITY,
            v1: f64::NEG_INFINITY,
        };
        for segment in &segments {
            for point in [segment.a, segment.b] {
                bounds.u0 = bounds.u0.min(point.u);
                bounds.v0 = bounds.v0.min(point.v);
                bounds.u1 = bounds.u1.max(point.u);
                bounds.v1 = bounds.v1.max(point.v);
            }
        }
        let longer = (bounds.u1 - bounds.u0).max(bounds.v1 - bounds.v0) + 2.0 * margin_units;
        let cell_side = longer / GRID_CELLS as f64;
        let grid = bounds.grown(margin_units + cell_side);
        let count = |extent: f64| ((extent / cell_side).ceil() as usize).max(1);
        let columns = count(grid.u1 - grid.u0);
        let rows = count(grid.v1 - grid.v0);

        let mut outline = Self {
            segments,
            margin_m,
            margin_units,
            grid,
            cell_side,
            columns,
            rows,
            cells: Vec::new(),
            cell_segments: Vec::new(),
            row_segments: vec![Vec::new(); rows],
        };
        outline.build_index();
        Ok(outline)
    }

    /// Reads an outline from GeoJSON: a Polygon or a MultiPolygon, bare or
    /// as the geometry of a Feature or of the one Feature of a
    /// FeatureCollection, in longitude and latitude (WGS 84).
    pub fn from_geojson(text: &str, margin_m: f64) -> Result<Self, OutlineError> {
        let bad = |what: &str| OutlineError(format!("not an outline: {what}"));
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| bad(&e.to_string()))?;
        let mut geometry = &value;
        if geometry["type"] == "FeatureCollection" {
            let features = geometry["features"]
                .as_array()
                .ok_or_else(|| bad("a FeatureCollection without features"))?;
            let [feature] = features.as_slice() else {
                return Err(bad("a FeatureCollection with other than one feature"));
            };
            geometry = feature;
        }
        if geometry["type"] == "Feature" {
            geometry = &geometry["geometry"];
        }
        let ring = |value: &serde_json::Value| -> Result<Vec<(f64, f64)>, OutlineError> {
            value
                .as_array()
                .ok_or_else(|| bad("a ring that is not a list"))?
                .iter()
                .map(|position| match position.as_array().map(Vec::as_slice) {
                    Some([lon, lat, ..]) => lon
                        .as_f64()
                        .zip(lat.as_f64())
                        .ok_or_else(|| bad("a position that is not two numbers")),
                    _ => Err(bad("a position that is not two numbers")),
                })
                .collect()
        };
        let polygon = |value: &serde_json::Value| -> Result<Vec<Vec<(f64, f64)>>, OutlineError> {
            value
                .as_array()
                .ok_or_else(|| bad("a polygon that is not a list of rings"))?
                .iter()
                .map(ring)
                .collect()
        };
        let coordinates = &geometry["coordinates"];
        let rings = match geometry["type"].as_str() {
            Some("Polygon") => polygon(coordinates)?,
            Some("MultiPolygon") => {
                let mut rings = Vec::new();
                for part in coordinates
                    .as_array()
                    .ok_or_else(|| bad("a MultiPolygon that is not a list of polygons"))?
                {
                    rings.extend(polygon(part)?);
                }
                rings
            }
            _ => return Err(bad("the geometry is neither a Polygon nor a MultiPolygon")),
        };
        Self::new(&rings, margin_m)
    }

    /// The margin on each side of the outline, in metres.
    pub fn margin_m(&self) -> f64 {
        self.margin_m
    }

    /// How many segments the rings have.
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    fn column_of(&self, u: f64) -> isize {
        ((u - self.grid.u0) / self.cell_side).floor() as isize
    }

    fn row_of(&self, v: f64) -> isize {
        ((v - self.grid.v0) / self.cell_side).floor() as isize
    }

    /// The cells under `rect`, as ranges of columns and rows, if any is.
    fn cells_under(
        &self,
        rect: Rect,
    ) -> Option<(
        std::ops::RangeInclusive<usize>,
        std::ops::RangeInclusive<usize>,
    )> {
        if !rect.overlaps(self.grid) {
            return None;
        }
        let clamp = |i: isize, count: usize| i.clamp(0, count as isize - 1) as usize;
        Some((
            clamp(self.column_of(rect.u0), self.columns)
                ..=clamp(self.column_of(rect.u1), self.columns),
            clamp(self.row_of(rect.v0), self.rows)..=clamp(self.row_of(rect.v1), self.rows),
        ))
    }

    fn build_index(&mut self) {
        let mut reaching: Vec<Vec<u32>> = vec![Vec::new(); self.columns * self.rows];
        for (number, segment) in self.segments.iter().enumerate() {
            let number = number as u32;
            let (north, south) = segment.v_span();
            let rect = Rect {
                u0: segment.a.u.min(segment.b.u),
                v0: north,
                u1: segment.a.u.max(segment.b.u),
                v1: south,
            };
            let reach = rect.grown(self.margin_units);
            if let Some((columns, rows)) = self.cells_under(reach) {
                for row in rows {
                    for column in columns.clone() {
                        let cell = Rect {
                            u0: self.grid.u0 + column as f64 * self.cell_side,
                            v0: self.grid.v0 + row as f64 * self.cell_side,
                            u1: self.grid.u0 + (column + 1) as f64 * self.cell_side,
                            v1: self.grid.v0 + (row + 1) as f64 * self.cell_side,
                        };
                        if segment.meets(cell.grown(self.margin_units)) {
                            reaching[row * self.columns + column].push(number);
                        }
                    }
                }
            }
            let first = self.row_of(north).clamp(0, self.rows as isize - 1) as usize;
            let last = self.row_of(south).clamp(0, self.rows as isize - 1) as usize;
            for row in first..=last {
                self.row_segments[row].push(number);
            }
        }
        let mut cells = Vec::with_capacity(reaching.len());
        for (index, segments) in reaching.into_iter().enumerate() {
            if segments.is_empty() {
                let (row, column) = (index / self.columns, index % self.columns);
                let middle = Point {
                    u: self.grid.u0 + (column as f64 + 0.5) * self.cell_side,
                    v: self.grid.v0 + (row as f64 + 0.5) * self.cell_side,
                };
                cells.push(if self.contains(middle) {
                    Cell::Inside
                } else {
                    Cell::Outside
                });
            } else {
                cells.push(Cell::Edge {
                    first: self.cell_segments.len() as u32,
                    count: segments.len() as u32,
                });
                self.cell_segments.extend(segments);
            }
        }
        self.cells = cells;
    }

    /// The places where the line of `v` crosses the rings, west to east.
    fn crossings(&self, v: f64) -> Vec<f64> {
        let row = self.row_of(v);
        if row < 0 || row >= self.rows as isize {
            return Vec::new();
        }
        let mut crossings: Vec<f64> = self.row_segments[row as usize]
            .iter()
            .filter_map(|&number| self.segments[number as usize].crossing(v))
            .collect();
        crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        crossings
    }

    /// Whether a point is inside the outline.
    pub(crate) fn contains(&self, point: Point) -> bool {
        let row = self.row_of(point.v);
        if row < 0 || row >= self.rows as isize {
            return false;
        }
        let east = self.row_segments[row as usize]
            .iter()
            .filter_map(|&number| self.segments[number as usize].crossing(point.v))
            .filter(|&u| u > point.u)
            .count();
        east % 2 == 1
    }

    /// Whether a position in degrees is inside the outline.
    pub fn contains_position(&self, lon: f64, lat: f64) -> bool {
        self.contains(mercator::project(lon, lat))
    }

    /// Where the square `rect` lies: wholly inside, wholly outside, or
    /// reached by the band.
    ///
    /// The band is measured as a square around the square here, not as a
    /// rounded one: a tile within the margin of a corner of the outline
    /// diagonally may be called an edge tile where it is not quite. That
    /// costs a cut that removes nothing, never a pixel of white.
    pub fn side_of(&self, rect: Rect) -> Side {
        let reach = rect.grown(self.margin_units);
        let Some((columns, rows)) = self.cells_under(reach) else {
            return Side::Outside;
        };
        let mut uniform = None;
        let mut near = false;
        for row in rows {
            for column in columns.clone() {
                match self.cells[row * self.columns + column] {
                    Cell::Edge { first, count } => {
                        near = true;
                        let first = first as usize;
                        let reaching = &self.cell_segments[first..first + count as usize];
                        if reaching
                            .iter()
                            .any(|&number| self.segments[number as usize].meets(reach))
                        {
                            return Side::Edge;
                        }
                    }
                    Cell::Inside => uniform = Some(Side::Inside),
                    Cell::Outside => uniform = Some(Side::Outside),
                }
            }
        }
        // No segment comes near, so the whole square is on one side. Cells
        // that no segment reaches all say which; under a square that only
        // cells with segments lie, its middle says.
        match (near, uniform) {
            (false, Some(side)) => side,
            _ if self.contains(rect.centre()) => Side::Inside,
            _ => Side::Outside,
        }
    }

    /// Where each pixel of a picture of `width` by `height` pixels that
    /// covers `rect` lies, row by row from the north, each row from the
    /// west. A pixel is taken at its middle.
    pub(crate) fn places(&self, rect: Rect, width: usize, height: usize) -> Vec<Place> {
        // The margin in world fractions at the middle of the square, and
        // never more than the index was built for.
        let latitude = mercator::latitude(rect.centre().v);
        let margin = (self.margin_m * mercator::units_per_metre(latitude)).min(self.margin_units);
        let reach = rect.grown(margin);
        let mut near: Vec<u32> = Vec::new();
        if let Some((columns, rows)) = self.cells_under(reach) {
            for row in rows {
                for column in columns.clone() {
                    if let Cell::Edge { first, count } = self.cells[row * self.columns + column] {
                        let first = first as usize;
                        near.extend(&self.cell_segments[first..first + count as usize]);
                    }
                }
            }
        }
        near.sort_unstable();
        near.dedup();
        near.retain(|&number| self.segments[number as usize].meets(reach));

        let pixel_u = (rect.u1 - rect.u0) / width as f64;
        let pixel_v = (rect.v1 - rect.v0) / height as f64;
        let mut places = Vec::with_capacity(width * height);
        let mut band = vec![false; width];
        for j in 0..height {
            let v = rect.v0 + (j as f64 + 0.5) * pixel_v;
            band.fill(false);
            for &number in &near {
                let segment = &self.segments[number as usize];
                let (north, south) = segment.v_span();
                if v < north - margin || v > south + margin {
                    continue;
                }
                if let Some((west, east)) = segment.reach(v, margin) {
                    // The pixels whose middles lie in the stretch.
                    let first = ((west - rect.u0) / pixel_u - 0.5).ceil().max(0.0);
                    let last = ((east - rect.u0) / pixel_u - 0.5).floor();
                    if last < 0.0 || first >= width as f64 {
                        continue;
                    }
                    let last = (last as usize).min(width - 1);
                    for flag in &mut band[first as usize..=last] {
                        *flag = true;
                    }
                }
            }
            let crossings = self.crossings(v);
            let mut west_of = 0;
            for (i, &in_band) in band.iter().enumerate() {
                let u = rect.u0 + (i as f64 + 0.5) * pixel_u;
                while west_of < crossings.len() && crossings[west_of] <= u {
                    west_of += 1;
                }
                let inside = (crossings.len() - west_of) % 2 == 1;
                places.push(match (in_band, inside) {
                    (true, _) => Place::Band,
                    (false, true) => Place::Inside,
                    (false, false) => Place::Outside,
                });
            }
        }
        places
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square of about 22 by 15 km around Innsbruck, with a square hole
    /// of about 4.5 by 3 km in it.
    fn square_with_a_hole() -> Outline {
        let outer = vec![(11.2, 47.2), (11.5, 47.2), (11.5, 47.335), (11.2, 47.335)];
        let hole = vec![(11.3, 47.25), (11.36, 47.25), (11.36, 47.28), (11.3, 47.28)];
        Outline::new(&[outer, hole], 50.0).unwrap()
    }

    #[test]
    fn a_point_is_inside_between_the_rings() {
        let outline = square_with_a_hole();
        assert!(outline.contains_position(11.25, 47.3));
        assert!(outline.contains_position(11.45, 47.21));
        // In the hole, and beyond the outer ring.
        assert!(!outline.contains_position(11.33, 47.265));
        assert!(!outline.contains_position(11.1, 47.3));
        assert!(!outline.contains_position(11.3, 47.4));
        assert!(!outline.contains_position(11.3, 46.0));
    }

    #[test]
    fn a_ring_may_or_may_not_repeat_its_first_position() {
        let open = vec![(11.2, 47.2), (11.5, 47.2), (11.5, 47.3)];
        let mut closed = open.clone();
        closed.push(open[0]);
        let a = Outline::new(&[open], 50.0).unwrap();
        let b = Outline::new(&[closed], 50.0).unwrap();
        assert_eq!(a.segment_count(), 3);
        assert_eq!(b.segment_count(), 3);
    }

    #[test]
    fn what_is_no_outline_is_refused() {
        assert!(Outline::new(&[], 50.0).is_err());
        assert!(Outline::new(&[vec![(11.0, 47.0), (12.0, 47.0)]], 50.0).is_err());
        let off = vec![(11.0, 47.0), (12.0, 47.0), (200.0, 47.0)];
        assert!(Outline::new(&[off], 50.0).is_err());
        let fine = vec![(11.0, 47.0), (12.0, 47.0), (12.0, 48.0)];
        assert!(Outline::new(std::slice::from_ref(&fine), 0.0).is_err());
        assert!(Outline::new(&[fine], f64::NAN).is_err());
        for text in [
            "",
            "{}",
            r#"{"type":"Point","coordinates":[11,47]}"#,
            r#"{"type":"FeatureCollection","features":[]}"#,
            r#"{"type":"Polygon","coordinates":[[[11,47],[12,"47"],[12,48]]]}"#,
        ] {
            assert!(Outline::from_geojson(text, 50.0).is_err(), "{text}");
        }
    }

    #[test]
    fn geojson_is_read_in_each_of_its_shapes() {
        let polygon = r#"{"type":"Polygon","coordinates":[[[11,47],[12,47],[12,48],[11,47]]]}"#;
        let feature = format!(r#"{{"type":"Feature","properties":{{}},"geometry":{polygon}}}"#);
        let collection = format!(r#"{{"type":"FeatureCollection","features":[{feature}]}}"#);
        let multi = r#"{"type":"MultiPolygon","coordinates":[[[[11,47],[12,47],[12,48],[11,47]]],
            [[[13,47],[14,47],[14,48],[13,47]]]]}"#;
        for text in [polygon, &feature, &collection] {
            let outline = Outline::from_geojson(text, 50.0).unwrap();
            assert!(outline.contains_position(11.9, 47.1));
            assert!(!outline.contains_position(13.9, 47.1));
        }
        let outline = Outline::from_geojson(multi, 50.0).unwrap();
        assert!(outline.contains_position(11.9, 47.1));
        assert!(outline.contains_position(13.9, 47.1));
        assert!(!outline.contains_position(12.5, 47.5));
    }

    #[test]
    fn a_square_is_inside_outside_or_on_the_edge() {
        let outline = square_with_a_hole();
        let square = |west: f64, south: f64, east: f64, north: f64| {
            outline.side_of(Rect::of_degrees(west, south, east, north))
        };
        assert_eq!(square(11.22, 47.29, 11.24, 47.31), Side::Inside);
        assert_eq!(square(10.0, 47.0, 10.5, 47.1), Side::Outside);
        // In the hole, well away from its rim.
        assert_eq!(square(11.32, 47.26, 11.34, 47.27), Side::Outside);
        // Across the rim of the hole, and across the outer ring.
        assert_eq!(square(11.35, 47.26, 11.37, 47.27), Side::Edge);
        assert_eq!(square(11.49, 47.3, 11.51, 47.31), Side::Edge);
        // Around the whole outline.
        assert_eq!(square(10.0, 46.0, 13.0, 48.0), Side::Edge);
        // Outside, but within the margin: 30 m east of the eastern side.
        let east = 11.5 + 30.0 / (111_320.0 * 47.3_f64.to_radians().cos());
        assert_eq!(square(east, 47.29, east + 0.001, 47.30), Side::Edge);
        // And 100 m east, which is beyond it.
        let east = 11.5 + 100.0 / (111_320.0 * 47.3_f64.to_radians().cos());
        assert_eq!(square(east, 47.29, east + 0.001, 47.30), Side::Outside);
    }

    #[test]
    fn a_pixel_is_inside_in_the_band_or_outside() {
        let outline = square_with_a_hole();
        // A square of about 250 m around the eastern side at 47.3°: the
        // side runs down its middle.
        let half = 0.0017;
        let rect = Rect::of_degrees(11.5 - half, 47.299, 11.5 + half, 47.301);
        let places = outline.places(rect, 256, 2);
        let row = &places[..256];
        // About 128 m on each side of the line, 50 m of it the band:
        // some 50 pixels on each side.
        let band = row.iter().filter(|p| **p == Place::Band).count();
        assert!((95..=105).contains(&band), "{band} pixels in the band");
        assert_eq!(row[0], Place::Inside);
        assert_eq!(row[255], Place::Outside);
        // West of the band inside, east of it outside, nothing between.
        let first = row.iter().position(|p| *p == Place::Band).unwrap();
        let last = row.iter().rposition(|p| *p == Place::Band).unwrap();
        assert!(row[..first].iter().all(|p| *p == Place::Inside));
        assert!(row[first..=last].iter().all(|p| *p == Place::Band));
        assert!(row[last + 1..].iter().all(|p| *p == Place::Outside));
        assert_eq!(&places[256..], row);
    }

    #[test]
    fn the_band_around_a_corner_is_round() {
        // Within 50 m of the corner diagonally is the band; 45 m east and
        // 45 m north of it is 64 m away, and not. Inside, 60 m from each
        // side is out of the band.
        let outline = square_with_a_hole();
        let metre_lon = 1.0 / (111_320.0 * 47.335_f64.to_radians().cos());
        let metre_lat = 1.0 / 111_200.0;
        let at = |east: f64, north: f64| {
            let lon = 11.5 + east * metre_lon;
            let lat = 47.335 + north * metre_lat;
            let rect = Rect::of_degrees(lon - 1e-7, lat - 1e-7, lon + 1e-7, lat + 1e-7);
            outline.places(rect, 1, 1)[0]
        };
        assert_eq!(at(30.0, 30.0), Place::Band);
        assert_eq!(at(45.0, 45.0), Place::Outside);
        assert_eq!(at(-60.0, -60.0), Place::Inside);
        assert_eq!(at(-45.0, -45.0), Place::Band);
        assert_eq!(at(0.0, 0.0), Place::Band);
    }

    #[test]
    fn a_segment_meets_a_box_it_crosses_and_no_other() {
        let segment = Segment {
            a: Point { u: 0.0, v: 0.0 },
            b: Point { u: 1.0, v: 1.0 },
        };
        let rect = |u0, v0, u1, v1| Rect { u0, v0, u1, v1 };
        assert!(segment.meets(rect(0.4, 0.4, 0.6, 0.6)));
        assert!(segment.meets(rect(0.9, 0.0, 1.1, 2.0)));
        assert!(!segment.meets(rect(0.6, 0.0, 1.0, 0.4)));
        assert!(!segment.meets(rect(2.0, 2.0, 3.0, 3.0)));
    }
}
