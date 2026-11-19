//! A set of airspaces that can be asked questions.
//!
//! The cockpit asks once a second which airspaces are around the pilot. With
//! a few thousand airspaces and tens of thousands of corners, looking at
//! every one each time would be wasteful, so the set is sorted once into a
//! grid of cells of a tenth of a degree. A question then looks only at the
//! airspaces whose bounding box touches the cells it concerns.
//!
//! Every answer is a pure function of the set and the question: no clock, no
//! I/O, the same result on the phone, in the browser and on the server.

use std::collections::BTreeMap;

use venti_domain::{Airspace, AirspaceId, BoundingBox, GeoPoint, VerticalLimit};

use crate::geometry::{M_PER_DEG, contains, inside_stretches, nearest_on_boundary};
use crate::vertical::{
    Altitudes, VerticalRelation, VerticalStatus, vertical_status, widest_range_m,
};

/// The side of a grid cell in degrees, unless the set covers so much of the
/// globe that this would make too many cells.
const SMALLEST_CELL_DEG: f64 = 0.1;

/// Most cells along one side of the grid.
const MOST_CELLS_PER_SIDE: f64 = 256.0;

/// An airspace touching more cells than this is kept in a list of its own
/// and looked at on every question. That is a handful of flight information
/// regions, and it keeps a hostile, world-sized airspace from filling memory.
const MOST_CELLS_PER_AIRSPACE: usize = 1024;

/// A set of airspaces, sorted for fast answers.
#[derive(Debug, Clone)]
pub struct AirspaceIndex {
    airspaces: Vec<Airspace>,
    boxes: Vec<BoundingBox>,
    by_id: BTreeMap<AirspaceId, usize>,
    grid: Grid,
    /// Airspaces too large for the grid.
    everywhere: Vec<usize>,
}

#[derive(Debug, Clone)]
struct Grid {
    south: f64,
    west: f64,
    cell_deg: f64,
    rows: usize,
    columns: usize,
    cells: Vec<Vec<usize>>,
}

/// What an index holds, for reporting and for judging its cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexStats {
    /// Airspaces in the set.
    pub airspaces: usize,
    /// Corners of all their rings together.
    pub points: usize,
    /// Cells of the grid.
    pub cells: usize,
    /// Entries in those cells: how often an airspace is listed.
    pub cell_entries: usize,
    /// A close estimate of the memory the index occupies, in bytes: the
    /// records, their corners, their texts and the grid.
    pub bytes: usize,
}

/// An airspace the pilot is horizontally inside of, and where the pilot is
/// in relation to its floor and ceiling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirspaceAt<'a> {
    /// The airspace.
    pub airspace: &'a Airspace,
    /// Below it, inside it or above it, and by how much.
    pub vertical: VerticalStatus,
}

/// Where a position is in relation to the outline of an airspace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Horizontal {
    /// Whether the position is inside the outline. On the outline counts as inside.
    pub inside: bool,
    /// Metres over the ground to the nearest point of the outline: the
    /// distance to the airspace from outside, the distance to the nearest
    /// way out from inside.
    pub distance_m: f64,
    /// The direction to that point, degrees clockwise from true north.
    pub bearing_deg: f64,
    /// That point.
    pub nearest: GeoPoint,
}

impl Horizontal {
    /// Metres of ground between the position and the airspace: zero inside it.
    pub fn separation_m(&self) -> f64 {
        if self.inside { 0.0 } else { self.distance_m }
    }
}

/// An airspace near the pilot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nearby<'a> {
    /// The airspace.
    pub airspace: &'a Airspace,
    /// Where the pilot is in relation to its outline.
    pub horizontal: Horizontal,
    /// Where the pilot is in relation to its floor and ceiling.
    pub vertical: VerticalStatus,
}

/// A band of altitudes above mean sea level, in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AltitudeBand {
    /// The bottom of the band.
    pub bottom_m: f64,
    /// The top of the band.
    pub top_m: f64,
}

/// A stretch of a path that lies under, in or over an airspace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionSpan<'a> {
    /// The airspace.
    pub airspace: &'a Airspace,
    /// Where along the path the stretch begins, in metres from its start.
    pub from_m: f64,
    /// Where it ends.
    pub to_m: f64,
    /// The floor of the airspace, as published.
    pub lower: VerticalLimit,
    /// Its ceiling, as published.
    pub upper: VerticalLimit,
}

impl AirspaceIndex {
    /// Sorts a set of airspaces for asking.
    ///
    /// The records are taken as they are. Checking them is the job of
    /// whoever read them: the pipeline for a data source, the reader for a
    /// file.
    pub fn new(airspaces: Vec<Airspace>) -> Self {
        let boxes: Vec<BoundingBox> = airspaces.iter().map(Airspace::bounds).collect();
        let mut by_id = BTreeMap::new();
        for (position, airspace) in airspaces.iter().enumerate() {
            by_id.entry(airspace.id.clone()).or_insert(position);
        }

        let mut grid = Grid::around(&boxes);
        let mut everywhere = Vec::new();
        for (position, bounds) in boxes.iter().enumerate() {
            let (rows, columns) = grid.range(bounds);
            if rows.len() * columns.len() > MOST_CELLS_PER_AIRSPACE {
                everywhere.push(position);
                continue;
            }
            for row in rows {
                for column in columns.clone() {
                    grid.cells[row * grid.columns + column].push(position);
                }
            }
        }

        Self {
            airspaces,
            boxes,
            by_id,
            grid,
            everywhere,
        }
    }

    /// How many airspaces the set holds.
    pub fn len(&self) -> usize {
        self.airspaces.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.airspaces.is_empty()
    }

    /// Every airspace of the set, in the order it was given.
    pub fn iter(&self) -> impl Iterator<Item = &Airspace> {
        self.airspaces.iter()
    }

    /// The airspace with this identifier. If two share one, the first.
    pub fn get(&self, id: &AirspaceId) -> Option<&Airspace> {
        self.by_id
            .get(id)
            .map(|&position| &self.airspaces[position])
    }

    /// What the index holds and how much memory it takes.
    pub fn stats(&self) -> IndexStats {
        let word = size_of::<usize>();
        let mut points = 0;
        let mut texts = 0;
        for airspace in &self.airspaces {
            let geometry = &airspace.geometry;
            points += geometry.exterior().len();
            points += geometry.holes().iter().map(Vec::len).sum::<usize>();
            texts += airspace.name.len() + 2 * airspace.id.to_string().len();
            texts += airspace.remarks.as_ref().map_or(0, String::len);
            texts += airspace
                .activity
                .hours_remarks
                .as_ref()
                .map_or(0, String::len);
            texts += airspace.frequencies.len() * 64;
            texts += airspace.activity.hours.len() * 48;
        }
        let cell_entries: usize = self.grid.cells.iter().map(Vec::len).sum();
        let bytes = self.airspaces.len() * (size_of::<Airspace>() + size_of::<BoundingBox>())
            + points * size_of::<GeoPoint>()
            + texts
            + self.by_id.len() * (size_of::<AirspaceId>() + word)
            + self.grid.cells.len() * size_of::<Vec<usize>>()
            + (cell_entries + self.everywhere.len()) * word;
        IndexStats {
            airspaces: self.airspaces.len(),
            points,
            cells: self.grid.cells.len(),
            cell_entries,
            bytes,
        }
    }

    /// The airspaces that contain a position horizontally, each with where
    /// the pilot is in relation to its floor and ceiling.
    ///
    /// Every airspace is listed whose outline contains the position, at
    /// whatever height it lies, so the result also tells what is above and
    /// below the pilot. What is unknown is taken toward the pilot: see
    /// [`vertical_status`]. The airspaces the pilot is in come first, then
    /// the others by how close they are in height.
    pub fn at(&self, position: GeoPoint, altitudes: &Altitudes) -> Vec<AirspaceAt<'_>> {
        let mut found: Vec<AirspaceAt<'_>> = self
            .grid
            .cell(position)
            .into_iter()
            .flatten()
            .chain(&self.everywhere)
            .filter(|&&candidate| self.boxes[candidate].contains(position))
            .map(|&candidate| &self.airspaces[candidate])
            .filter(|airspace| contains(&airspace.geometry, position))
            .map(|airspace| AirspaceAt {
                airspace,
                vertical: vertical_status(airspace, altitudes),
            })
            .collect();
        found.sort_by(|a, b| {
            let clear =
                |entry: &AirspaceAt<'_>| entry.vertical.relation != VerticalRelation::Inside;
            clear(a)
                .cmp(&clear(b))
                .then(a.vertical.separation_m.total_cmp(&b.vertical.separation_m))
                .then_with(|| a.airspace.id.cmp(&b.airspace.id))
        });
        found
    }

    /// The airspaces around a position: for each, the distance and direction
    /// to its outline.
    ///
    /// Listed are the airspaces whose outline is within `radius_m` of the
    /// position and every airspace whose outline contains it, however far
    /// its edge is. Of those, only the ones the pilot is at a height to
    /// enter: inside their height band or within `vertical_margin_m` of it,
    /// with what is unknown taken toward the pilot. The ones the pilot is
    /// horizontally inside come first, then the others, nearest first.
    pub fn nearby(
        &self,
        position: GeoPoint,
        altitudes: &Altitudes,
        radius_m: f64,
        vertical_margin_m: f64,
    ) -> Vec<Nearby<'_>> {
        // A distance that is not a number reaches nowhere.
        let at_least_zero = |metres: f64| {
            if metres.is_finite() {
                metres.max(0.0)
            } else {
                0.0
            }
        };
        let (radius_m, vertical_margin_m) =
            (at_least_zero(radius_m), at_least_zero(vertical_margin_m));
        let mut found: Vec<Nearby<'_>> = self
            .near(position, radius_m)
            .into_iter()
            .filter_map(|airspace| {
                let vertical = vertical_status(airspace, altitudes);
                if vertical.separation_m > vertical_margin_m {
                    return None;
                }
                let horizontal = horizontal(airspace, position)?;
                (horizontal.separation_m() <= radius_m).then_some(Nearby {
                    airspace,
                    horizontal,
                    vertical,
                })
            })
            .collect();
        found.sort_by(|a, b| {
            a.horizontal
                .separation_m()
                .total_cmp(&b.horizontal.separation_m())
                .then_with(|| a.airspace.id.cmp(&b.airspace.id))
        });
        found
    }

    /// The airspaces whose bounding box comes within `radius_m` of a
    /// position: the candidates for anything that happens near it.
    pub(crate) fn near(&self, position: GeoPoint, radius_m: f64) -> Vec<&Airspace> {
        let radius_m = if radius_m.is_finite() {
            radius_m.max(0.0)
        } else {
            0.0
        };
        let lat_reach = radius_m / M_PER_DEG;
        let shrink = position.lat().to_radians().cos().max(1e-6);
        let lon_reach = (lat_reach / shrink).min(360.0);
        let area = Area {
            south: position.lat() - lat_reach,
            north: position.lat() + lat_reach,
            west: position.lon() - lon_reach,
            east: position.lon() + lon_reach,
        };
        self.touching(&area)
    }

    /// The airspaces whose bounding box touches a box, and whose height may
    /// reach into a band of altitudes if one is given.
    ///
    /// This is the filter for drawing a map: cheap, and generous. An
    /// airspace is included when its box touches the box, without looking
    /// at its exact outline. With a band it is left out only when it lies
    /// certainly above or below it, wherever in the world and whatever the
    /// weather: a floor above ground or at a flight level is taken as low,
    /// and a ceiling as high, as it can be.
    pub fn in_box(&self, area: &BoundingBox, band: Option<AltitudeBand>) -> Vec<&Airspace> {
        let area = Area {
            south: area.south(),
            north: area.north(),
            west: area.west(),
            east: area.east(),
        };
        let mut found = self.touching(&area);
        if let Some(band) = band {
            found.retain(|airspace| {
                // A second floor ("but at least") only ever raises the
                // floor, so the published one is the lowest it can be.
                let (floor, _) = widest_range_m(&airspace.lower);
                let (_, ceiling) = widest_range_m(&airspace.upper);
                let raised = airspace
                    .upper_at_most
                    .as_ref()
                    .map_or(ceiling, |limit| widest_range_m(limit).1.max(ceiling));
                floor <= band.top_m && raised >= band.bottom_m
            });
        }
        found
    }

    /// The airspaces along a path, for a side view: where the path runs
    /// under, in or over each one.
    ///
    /// The path is the straight lines between its points. For every
    /// airspace whose outline it crosses, the result gives the stretches
    /// that lie within the outline, as distances from the start of the path,
    /// with the floor and ceiling as published. Placing those on a height
    /// scale is left to the caller, who has the terrain along the path:
    /// [`resolve`](crate::resolve) does it for one point. Stretches are in
    /// order of where they begin.
    pub fn cross_section(&self, path: &[GeoPoint]) -> Vec<SectionSpan<'_>> {
        let mut stretches: BTreeMap<usize, Vec<(f64, f64)>> = BTreeMap::new();
        let mut travelled = 0.0;
        for leg in path.windows(2) {
            let (from, to) = (leg[0], leg[1]);
            let length = from.distance_m(to);
            let area = Area {
                south: from.lat().min(to.lat()),
                north: from.lat().max(to.lat()),
                west: from.lon().min(to.lon()),
                east: from.lon().max(to.lon()),
            };
            for candidate in self.candidates(&area) {
                let airspace = &self.airspaces[candidate];
                for (start, end) in inside_stretches(&airspace.geometry, from, to) {
                    let (start, end) = (travelled + start * length, travelled + end * length);
                    let spans = stretches.entry(candidate).or_default();
                    match spans.last_mut() {
                        // Joins what a corner of the path had cut in two.
                        Some(last) if start - last.1 <= 1e-6 => last.1 = last.1.max(end),
                        _ => spans.push((start, end)),
                    }
                }
            }
            travelled += length;
        }

        let mut spans: Vec<SectionSpan<'_>> = stretches
            .into_iter()
            .flat_map(|(candidate, spans)| {
                let airspace = &self.airspaces[candidate];
                spans.into_iter().map(move |(from_m, to_m)| SectionSpan {
                    airspace,
                    from_m,
                    to_m,
                    lower: airspace.lower,
                    upper: airspace.upper,
                })
            })
            .collect();
        spans.sort_by(|a, b| {
            a.from_m
                .total_cmp(&b.from_m)
                .then_with(|| a.airspace.id.cmp(&b.airspace.id))
        });
        spans
    }

    fn touching(&self, area: &Area) -> Vec<&Airspace> {
        self.candidates(area)
            .into_iter()
            .map(|candidate| &self.airspaces[candidate])
            .collect()
    }

    /// The positions of the airspaces whose bounding box touches an area,
    /// in the order of the set.
    fn candidates(&self, area: &Area) -> Vec<usize> {
        let mut found: Vec<usize> = self.everywhere.clone();
        if let Some((rows, columns)) = self.grid.cells_touching(area) {
            for row in rows {
                for column in columns.clone() {
                    found.extend(&self.grid.cells[row * self.grid.columns + column]);
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found.retain(|&candidate| area.touches(&self.boxes[candidate]));
        found
    }
}

/// Where a position is in relation to the outline of one airspace.
///
/// `None` only if the nearest point of the outline cannot be placed on the
/// globe, which does not happen away from the poles.
pub fn horizontal(airspace: &Airspace, position: GeoPoint) -> Option<Horizontal> {
    let (nearest, distance_m, bearing_deg) = nearest_on_boundary(&airspace.geometry, position)?;
    Some(Horizontal {
        inside: contains(&airspace.geometry, position),
        distance_m,
        bearing_deg,
        nearest,
    })
}

/// A box in degrees that, unlike [`BoundingBox`], may reach past the edge
/// of the map: a search radius around a point near the antimeridian does.
#[derive(Debug, Clone, Copy)]
struct Area {
    south: f64,
    north: f64,
    west: f64,
    east: f64,
}

impl Area {
    fn touches(&self, bounds: &BoundingBox) -> bool {
        bounds.north() >= self.south
            && bounds.south() <= self.north
            && bounds.east() >= self.west
            && bounds.west() <= self.east
    }
}

impl Grid {
    /// A grid over everything the boxes cover.
    fn around(boxes: &[BoundingBox]) -> Self {
        let Some(first) = boxes.first() else {
            return Self {
                south: 0.0,
                west: 0.0,
                cell_deg: SMALLEST_CELL_DEG,
                rows: 0,
                columns: 0,
                cells: Vec::new(),
            };
        };
        let all = boxes.iter().fold(*first, |all, bounds| all.cover(bounds));
        let (height, width) = (all.north() - all.south(), all.east() - all.west());
        let cell_deg = SMALLEST_CELL_DEG
            .max(height / MOST_CELLS_PER_SIDE)
            .max(width / MOST_CELLS_PER_SIDE);
        let count = |span: f64| {
            ((span / cell_deg).floor() as usize + 1).min(MOST_CELLS_PER_SIDE as usize + 1)
        };
        let (rows, columns) = (count(height), count(width));
        Self {
            south: all.south(),
            west: all.west(),
            cell_deg,
            rows,
            columns,
            cells: vec![Vec::new(); rows * columns],
        }
    }

    /// The rows and columns a box inside the grid covers.
    fn range(&self, bounds: &BoundingBox) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let area = Area {
            south: bounds.south(),
            north: bounds.north(),
            west: bounds.west(),
            east: bounds.east(),
        };
        self.cells_touching(&area).unwrap_or((0..0, 0..0))
    }

    /// The rows and columns an area touches, if it touches the grid at all.
    fn cells_touching(
        &self,
        area: &Area,
    ) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
        if self.rows == 0 || self.columns == 0 {
            return None;
        }
        let first = |from: f64, origin: f64| ((from - origin) / self.cell_deg).floor().max(0.0);
        let last = |to: f64, origin: f64, count: usize| {
            ((to - origin) / self.cell_deg)
                .floor()
                .min(count as f64 - 1.0)
        };
        let (first_row, last_row) = (
            first(area.south, self.south),
            last(area.north, self.south, self.rows),
        );
        let (first_column, last_column) = (
            first(area.west, self.west),
            last(area.east, self.west, self.columns),
        );
        // Also false when a bound is not a number.
        if !(first_row <= last_row && first_column <= last_column) {
            return None;
        }
        Some((
            first_row as usize..last_row as usize + 1,
            first_column as usize..last_column as usize + 1,
        ))
    }

    /// The airspaces listed in the cell a position falls in.
    fn cell(&self, position: GeoPoint) -> Option<&Vec<usize>> {
        let area = Area {
            south: position.lat(),
            north: position.lat(),
            west: position.lon(),
            east: position.lon(),
        };
        let (rows, columns) = self.cells_touching(&area)?;
        self.cells.get(rows.start * self.columns + columns.start)
    }
}

#[cfg(test)]
mod tests {
    use venti_domain::{
        Activity, AirspaceClass, AirspaceType, HeightUnit, Polygon, Validity, VerticalReference,
    };

    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    /// A rectangle between two corners, from `lower` to `upper`.
    fn airspace(
        id: &str,
        south_west: (f64, f64),
        north_east: (f64, f64),
        lower: VerticalLimit,
        upper: VerticalLimit,
    ) -> Airspace {
        let ((south, west), (north, east)) = (south_west, north_east);
        let ring = vec![
            point(south, west),
            point(south, east),
            point(north, east),
            point(north, west),
            point(south, west),
        ];
        Airspace {
            id: format!("test:{id}").parse().unwrap(),
            name: id.to_uppercase(),
            class: AirspaceClass::D,
            airspace_type: AirspaceType::Ctr,
            country: None,
            lower,
            upper,
            lower_at_least: None,
            upper_at_most: None,
            geometry: Polygon::new(ring, Vec::new()).unwrap(),
            activity: Activity::default(),
            frequencies: Vec::new(),
            remarks: None,
            validity: Validity::default(),
        }
    }

    fn msl(feet: f64) -> VerticalLimit {
        VerticalLimit::new(VerticalReference::MeanSeaLevel, feet, HeightUnit::Feet).unwrap()
    }

    /// A control zone from the ground to 5000 ft, a terminal area above it
    /// and beyond from 5000 ft to FL 125, and a far-away zone.
    fn innsbruck() -> AirspaceIndex {
        AirspaceIndex::new(vec![
            airspace(
                "ctr",
                (47.2, 11.2),
                (47.3, 11.5),
                VerticalLimit::ground(),
                msl(5000.0),
            ),
            airspace(
                "tma",
                (47.1, 11.0),
                (47.4, 11.7),
                msl(5000.0),
                VerticalLimit::flight_level(125).unwrap(),
            ),
            airspace(
                "far",
                (48.3, 11.7),
                (48.4, 11.9),
                VerticalLimit::ground(),
                msl(3500.0),
            ),
        ])
    }

    fn names(found: &[AirspaceAt<'_>]) -> Vec<String> {
        found
            .iter()
            .map(|entry| entry.airspace.name.clone())
            .collect()
    }

    #[test]
    fn the_airspaces_at_a_position_come_with_the_pilots_height_in_each() {
        let index = innsbruck();
        // Over the airfield at 1200 m: in the control zone, under the terminal area.
        let found = index.at(point(47.25, 11.35), &Altitudes::gps(1200.0));
        assert_eq!(names(&found), ["CTR", "TMA"]);
        assert_eq!(found[0].vertical.relation, VerticalRelation::Inside);
        assert_eq!(found[1].vertical.relation, VerticalRelation::Below);
        assert!((found[1].vertical.separation_m - 324.0).abs() < 1e-9);

        // At 2000 m: above the zone, in the terminal area.
        let found = index.at(point(47.25, 11.35), &Altitudes::gps(2000.0));
        assert_eq!(names(&found), ["TMA", "CTR"]);
        assert_eq!(found[1].vertical.relation, VerticalRelation::Above);
        assert!((found[1].vertical.separation_m - 476.0).abs() < 1e-9);

        // West of the zone only the terminal area is overhead.
        assert_eq!(
            names(&index.at(point(47.25, 11.1), &Altitudes::gps(1200.0))),
            ["TMA"]
        );
        // Away from everything, and off the edge of the data.
        assert!(
            index
                .at(point(47.8, 11.35), &Altitudes::gps(1200.0))
                .is_empty()
        );
        assert!(
            index
                .at(point(10.0, 100.0), &Altitudes::gps(1200.0))
                .is_empty()
        );
        assert!(
            index
                .at(point(-90.0, 180.0), &Altitudes::gps(1200.0))
                .is_empty()
        );
    }

    #[test]
    fn nearby_airspaces_are_listed_with_distance_and_direction() {
        let index = innsbruck();
        // 0.02 degrees of latitude north of the control zone, at its height.
        let north_of_zone = point(47.32, 11.35);
        let found = index.nearby(north_of_zone, &Altitudes::gps(1200.0), 5000.0, 0.0);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].airspace.name, "CTR");
        assert!(!found[0].horizontal.inside);
        assert!((found[0].horizontal.distance_m - 0.02 * M_PER_DEG).abs() < 0.01);
        assert!((found[0].horizontal.bearing_deg - 180.0).abs() < 1e-6);

        // The terminal area is overhead but 324 m higher: listed once the
        // margin reaches it, and first, because the pilot is under it.
        let found = index.nearby(north_of_zone, &Altitudes::gps(1200.0), 5000.0, 400.0);
        let names: Vec<&str> = found.iter().map(|n| n.airspace.name.as_str()).collect();
        assert_eq!(names, ["TMA", "CTR"]);
        assert!(found[0].horizontal.inside);
        // From inside, the distance is to the nearest way out: the northern edge.
        assert!((found[0].horizontal.distance_m - 0.08 * M_PER_DEG).abs() < 0.01);
        assert!(found[0].horizontal.bearing_deg.abs() < 1e-6);

        // A smaller radius leaves the zone out; the area overhead stays.
        let found = index.nearby(north_of_zone, &Altitudes::gps(1200.0), 1000.0, 400.0);
        assert_eq!(found.len(), 1);
        // Above everything, nothing is at the pilot's height.
        assert!(
            index
                .nearby(north_of_zone, &Altitudes::gps(5000.0), 5000.0, 0.0)
                .is_empty()
        );
        // A radius that is not a number finds only what is overhead.
        assert_eq!(
            index
                .nearby(north_of_zone, &Altitudes::gps(2000.0), f64::NAN, 0.0)
                .len(),
            1
        );
    }

    #[test]
    fn a_map_gets_what_touches_its_box_and_its_band_of_altitudes() {
        let index = innsbruck();
        let tirol = BoundingBox::new(47.0, 10.5, 47.5, 12.0).unwrap();
        let name = |found: Vec<&Airspace>| -> Vec<String> {
            found.iter().map(|a| a.name.clone()).collect()
        };

        assert_eq!(name(index.in_box(&tirol, None)), ["CTR", "TMA"]);
        let everything = BoundingBox::new(46.0, 10.0, 49.0, 13.0).unwrap();
        assert_eq!(name(index.in_box(&everything, None)), ["CTR", "TMA", "FAR"]);
        let nothing = BoundingBox::new(40.0, 0.0, 41.0, 1.0).unwrap();
        assert!(index.in_box(&nothing, None).is_empty());

        // Up to 1000 m only the zone, which starts at the ground.
        let low = AltitudeBand {
            bottom_m: 0.0,
            top_m: 1000.0,
        };
        assert_eq!(name(index.in_box(&tirol, Some(low))), ["CTR"]);
        // Between 2000 m and 3000 m only the terminal area.
        let high = AltitudeBand {
            bottom_m: 2000.0,
            top_m: 3000.0,
        };
        assert_eq!(name(index.in_box(&tirol, Some(high))), ["TMA"]);
        // Its ceiling is a flight level: placed as high as it can be, so a
        // band just above its nominal 3810 m still includes it.
        let above = AltitudeBand {
            bottom_m: 4000.0,
            top_m: 5000.0,
        };
        assert_eq!(name(index.in_box(&tirol, Some(above))), ["TMA"]);
        let far_above = AltitudeBand {
            bottom_m: 4500.0,
            top_m: 5000.0,
        };
        assert!(index.in_box(&tirol, Some(far_above)).is_empty());
    }

    #[test]
    fn a_path_is_cut_where_it_crosses_each_outline() {
        let index = innsbruck();
        // West to east along 47.25 N, with a corner in the middle of the zone.
        let path = [point(47.25, 10.9), point(47.25, 11.35), point(47.25, 11.9)];
        let spans = index.cross_section(&path);
        let degree = M_PER_DEG * 47.25_f64.to_radians().cos();

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].airspace.name, "TMA");
        assert!(
            (spans[0].from_m - 0.1 * degree).abs() < 5.0,
            "{}",
            spans[0].from_m
        );
        assert!((spans[0].to_m - 0.8 * degree).abs() < 5.0);
        // The corner in the path does not cut the zone in two.
        assert_eq!(spans[1].airspace.name, "CTR");
        assert!((spans[1].from_m - 0.3 * degree).abs() < 5.0);
        assert!((spans[1].to_m - 0.6 * degree).abs() < 5.0);
        assert_eq!(spans[1].lower, VerticalLimit::ground());
        assert_eq!(spans[1].upper, msl(5000.0));

        // A path that leaves an airspace and comes back gives two stretches.
        let there_and_back = [point(47.25, 11.1), point(47.5, 11.35), point(47.25, 11.6)];
        let spans = index.cross_section(&there_and_back);
        let tma: Vec<_> = spans.iter().filter(|s| s.airspace.name == "TMA").collect();
        assert_eq!(tma.len(), 2);
        assert_eq!(tma[0].from_m, 0.0);
        assert!(tma[0].to_m < tma[1].from_m);

        assert!(index.cross_section(&[]).is_empty());
        assert!(index.cross_section(&[point(47.25, 11.35)]).is_empty());
    }

    #[test]
    fn an_airspace_is_found_by_its_identifier() {
        let index = innsbruck();
        assert_eq!(index.len(), 3);
        assert!(!index.is_empty());
        assert_eq!(index.iter().count(), 3);
        let id: AirspaceId = "test:tma".parse().unwrap();
        assert_eq!(index.get(&id).unwrap().name, "TMA");
        assert!(index.get(&"test:none".parse().unwrap()).is_none());
        let stats = index.stats();
        assert_eq!((stats.airspaces, stats.points), (3, 15));
        assert!(stats.bytes > 0 && stats.cells > 0 && stats.cell_entries >= 3);
    }

    #[test]
    fn an_empty_index_answers_everything_with_nothing() {
        let index = AirspaceIndex::new(Vec::new());
        let here = point(47.0, 11.0);
        assert!(index.is_empty());
        assert!(index.at(here, &Altitudes::gps(1000.0)).is_empty());
        assert!(
            index
                .nearby(here, &Altitudes::gps(1000.0), 1e6, 1e6)
                .is_empty()
        );
        assert!(
            index
                .in_box(&BoundingBox::new(-90.0, -180.0, 90.0, 180.0).unwrap(), None)
                .is_empty()
        );
        assert!(index.cross_section(&[here, point(48.0, 12.0)]).is_empty());
    }

    #[test]
    fn airspaces_too_large_for_the_grid_are_still_found() {
        // A flight information region over a small zone: the region touches
        // more cells than the grid lists an airspace in.
        let index = AirspaceIndex::new(vec![
            airspace(
                "fir",
                (40.0, 0.0),
                (55.0, 20.0),
                VerticalLimit::ground(),
                VerticalLimit::unlimited(),
            ),
            airspace(
                "ctr",
                (47.2, 11.2),
                (47.3, 11.5),
                VerticalLimit::ground(),
                msl(5000.0),
            ),
        ]);
        assert_eq!(index.everywhere, [0]);
        let found = index.at(point(47.25, 11.35), &Altitudes::gps(1200.0));
        assert_eq!(found.len(), 2);
        let found = index.at(point(52.0, 5.0), &Altitudes::gps(1200.0));
        assert_eq!(names(&found), ["FIR"]);
        let spans = index.cross_section(&[point(47.25, 11.0), point(47.25, 12.0)]);
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn a_search_near_the_edge_of_the_map_does_not_panic() {
        let index = AirspaceIndex::new(vec![airspace(
            "edge",
            (-10.0, 179.0),
            (10.0, 180.0),
            VerticalLimit::ground(),
            VerticalLimit::unlimited(),
        )]);
        for (lat, lon) in [
            (0.0, 180.0),
            (0.0, -180.0),
            (90.0, 0.0),
            (-90.0, 0.0),
            (89.999, 179.999),
        ] {
            let here = point(lat, lon);
            let _ = index.at(here, &Altitudes::gps(1000.0));
            let _ = index.nearby(here, &Altitudes::gps(1000.0), 50_000.0, 500.0);
            let _ = index.cross_section(&[here, point(0.0, 0.0)]);
        }
        assert_eq!(
            index.at(point(0.0, 179.5), &Altitudes::gps(1000.0)).len(),
            1
        );
    }
}
