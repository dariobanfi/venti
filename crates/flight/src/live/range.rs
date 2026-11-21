//! How far the glider glides: the edge of what a straight glide reaches
//! over the ground as it is, in every direction.
//!
//! In each direction the glider's own glide, in the wind as last worked
//! out, is laid over the ground along that line. The edge is where it
//! comes down to a reserve over the ground: on the valley floor after a
//! long glide, or on a slope after a short one. Nothing here flies around
//! a ridge: what lies behind one counts as out of reach, although a pilot
//! may reach it by another way.
//!
//! The same edge answers two questions. In flight, [`range`]: how far the
//! glider gets from where it is, held over the ground by the reserve from
//! the start. Before a flight, [`reach`]: how far a glide gets from a
//! place on the map, a summit to walk up among them, where the glide
//! begins on the ground. There the reserve cannot be had at the start: it
//! grows from what the glider has over the ground at the start to the
//! whole reserve over the first [`RESERVE_GROWS_OVER_M`]. A slope that
//! falls away more slowly than that, together with the glide, leaves no
//! way in its direction.
//!
//! What neither knows: the air rising or sinking on the way, a wind that
//! changes with the height or the place, and ground finer than the tiles
//! and the steps it is read in.

use venti_domain::GeoPoint;

use super::navigation::{self, Glider};
use super::wind::Wind;
use crate::geodesy::Earth;
use crate::terrain::Terrain;

/// The edge is worked out in this many directions: every five degrees.
pub const DIRECTIONS: usize = 72;

/// The ground along each line is read this far apart, metres.
pub const STEP_M: f64 = 100.0;

/// In flight no glide is followed further than this, metres. The ground
/// that is held reaches some thirteen kilometres from the glider.
const AT_MOST_M: f64 = 20_000.0;

/// Over this many metres from the start the reserve a glide keeps grows
/// from what it has over the ground at the start to the whole reserve. A
/// glide begun on the ground at a summit with a reserve of 200 m and a
/// glide of 8 thus needs ground that falls away by a third of the way,
/// some 18°, as the slope of a take-off does: 200 m over 1000 m for the
/// reserve and 125 m for the glide. Ours, to verify with pilots.
pub const RESERVE_GROWS_OVER_M: f64 = 1000.0;

/// No ground is lower than this, metres: the shore of the Dead Sea lies at
/// −430 m. A glide before the flight is followed until it would be this
/// low, which over any ground ends it before.
const LOWEST_GROUND_M: f64 = -500.0;

/// Why the edge lies where it does, in one direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeEnd {
    /// The glide comes down to the reserve over the ground there.
    Ground,
    /// The ground that is held ends there, or the glide was followed as
    /// far as any is: the glider reaches at least this far.
    Unknown,
    /// The wind lets the glider make no way in this direction.
    NoWay,
}

/// The edge in one direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    /// The direction, degrees clockwise from north.
    pub bearing_deg: f64,
    /// How far the glide gets, metres over the ground.
    pub distance_m: f64,
    /// The place it gets to.
    pub position: GeoPoint,
    /// Why it ends there.
    pub end: EdgeEnd,
    /// How high the glider is there, metres above mean sea level.
    pub arrival_m: f64,
    /// The ground there, metres above mean sea level: between the two
    /// places read around it, taken as straight. `None` where it is not
    /// held.
    pub ground_m: Option<f64>,
}

/// The edge of what a straight glide reaches from a place and a height,
/// with `reserve_m` to spare over the ground at its end: one entry per
/// direction, clockwise from north. `None` when the ground under the
/// glider is not held, and when the glider is lower over it than the
/// reserve: there is then nothing to draw.
pub fn range(
    terrain: &Terrain,
    position: GeoPoint,
    altitude_m: f64,
    wind: Option<Wind>,
    glider: &Glider,
    reserve_m: f64,
) -> Option<Vec<Edge>> {
    let under = terrain.elevation_m(position)?;
    if altitude_m - under < reserve_m {
        return None;
    }
    Some(edges(
        terrain,
        position,
        altitude_m,
        under,
        wind,
        glider,
        reserve_m,
        |_| AT_MOST_M,
    ))
}

/// What a glide reaches from a place on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct Reach {
    /// The height the glide begins at, metres above mean sea level: the
    /// one asked for, or the ground's where that was lower.
    pub altitude_m: f64,
    /// The ground at the start, metres above mean sea level.
    pub ground_m: f64,
    /// The edge, one entry per direction, clockwise from north.
    pub edges: Vec<Edge>,
}

/// The edge of what a straight glide reaches from a place on the map,
/// begun at a height, on the ground or above it, with `reserve_m` over
/// the ground at its end and on the way, after the first
/// [`RESERVE_GROWS_OVER_M`]. A height under the ground there is taken
/// as the ground's. Each glide is followed until it comes down to the
/// reserve or the ground held ends. `None` when the ground at the place
/// is not held.
pub fn reach(
    terrain: &Terrain,
    position: GeoPoint,
    altitude_m: f64,
    wind: Option<Wind>,
    glider: &Glider,
    reserve_m: f64,
) -> Option<Reach> {
    let ground_m = terrain.elevation_m(position)?;
    let altitude_m = altitude_m.max(ground_m);
    let edges = edges(
        terrain,
        position,
        altitude_m,
        ground_m,
        wind,
        glider,
        reserve_m,
        |glide| glide * (altitude_m - LOWEST_GROUND_M),
    );
    Some(Reach {
        altitude_m,
        ground_m,
        edges,
    })
}

/// The edge in every direction. `at_most_m` says, for the glide over the
/// ground in a direction, how far to follow it at the most.
#[allow(clippy::too_many_arguments)]
fn edges(
    terrain: &Terrain,
    position: GeoPoint,
    altitude_m: f64,
    under: f64,
    wind: Option<Wind>,
    glider: &Glider,
    reserve_m: f64,
    at_most_m: impl Fn(f64) -> f64,
) -> Vec<Edge> {
    (0..DIRECTIONS)
        .map(|direction| {
            let bearing_deg = 360.0 * direction as f64 / DIRECTIONS as f64;
            let (distance_m, end, ground_m, glide) =
                match navigation::glide_over_ground(bearing_deg, wind, glider) {
                    None => (0.0, EdgeEnd::NoWay, Some(under), f64::INFINITY),
                    Some(glide) => {
                        let places = (at_most_m(glide) / STEP_M).floor().max(0.0) as usize + 1;
                        let ground = terrain.along(position, bearing_deg, STEP_M).take(places);
                        let (distance_m, end, ground_m) =
                            follow(ground, altitude_m, altitude_m - under, glide, reserve_m);
                        (distance_m, end, ground_m, glide)
                    }
                };
            Edge {
                bearing_deg,
                distance_m,
                position: Earth::Wgs84.destination(position, bearing_deg, distance_m),
                end,
                arrival_m: altitude_m - distance_m / glide,
                ground_m,
            }
        })
        .collect()
}

/// How far a glide gets over the ground along one line, why it ends, and
/// the ground where it does. `ground` holds the ground every [`STEP_M`]
/// from the start on; `start_clear_m` is how high over it the glide
/// begins.
fn follow(
    ground: impl Iterator<Item = Option<f64>>,
    altitude_m: f64,
    start_clear_m: f64,
    glide: f64,
    reserve_m: f64,
) -> (f64, EdgeEnd, Option<f64>) {
    let start_clear_m = start_clear_m.max(0.0);
    // How far over the reserve the line is at each place, the reserve
    // growing from what the glide has at its start.
    let over = |place: usize, ground: f64| {
        let along = place as f64 * STEP_M;
        let reserve = reserve_m.min(start_clear_m + reserve_m * along / RESERVE_GROWS_OVER_M);
        altitude_m - along / glide - ground - reserve
    };
    let mut before: Option<(f64, f64)> = None;
    let mut last = 0;
    for (place, height) in ground.enumerate() {
        last = place;
        let Some(height) = height else {
            // The ground held ends here: the glide got as far as the
            // place before.
            return (
                place.saturating_sub(1) as f64 * STEP_M,
                EdgeEnd::Unknown,
                before.map(|(_, ground)| ground),
            );
        };
        let clear = over(place, height);
        if clear < 0.0 {
            // Between two places the line and the ground are taken as
            // straight.
            let reached = match before {
                Some((before, ground)) if before > 0.0 => {
                    let share = before / (before - clear);
                    (
                        (place - 1) as f64 * STEP_M + STEP_M * share,
                        ground + (height - ground) * share,
                    )
                }
                Some((_, ground)) => ((place - 1) as f64 * STEP_M, ground),
                None => (0.0, height),
            };
            return (reached.0, EdgeEnd::Ground, Some(reached.1));
        }
        before = Some((clear, height));
    }
    (
        last as f64 * STEP_M,
        EdgeEnd::Unknown,
        before.map(|(_, ground)| ground),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TileAddress;

    /// A glider that trims at 10 m/s with a glide of 8.
    fn glider() -> Glider {
        Glider {
            trim_speed_ms: 10.0,
            glide_ratio: 8.0,
        }
    }

    /// A glide in flight over the ground of a line, as [`range`] follows
    /// it: the reserve held from the start.
    fn along(
        ground: &[Option<f64>],
        altitude_m: f64,
        glide: f64,
        reserve_m: f64,
    ) -> (f64, EdgeEnd) {
        let start = ground[0].unwrap();
        let (distance_m, end, _) = follow(
            ground.iter().copied(),
            altitude_m,
            altitude_m - start,
            glide,
            reserve_m,
        );
        (distance_m, end)
    }

    /// One tile of zoom 10, 1024 pixels a side (some 26 m a pixel at
    /// this latitude), whose ground is given for each pixel's middle as
    /// metres east and north of the tile's middle; and that middle.
    pub(crate) fn ground(height: impl Fn(f64, f64) -> f64) -> (Terrain, GeoPoint) {
        use std::f64::consts::PI;
        let (z, side) = (10_u8, 1024_u32);
        let address = TileAddress::holding(GeoPoint::new(47.6642, 11.7967).unwrap(), z).unwrap();
        let tiles = f64::from(1_u32 << z);
        let place = |x: f64, y: f64| {
            let lon = x / tiles * 360.0 - 180.0;
            let lat = (PI * (1.0 - 2.0 * y / tiles)).sinh().atan().to_degrees();
            GeoPoint::new(lat, lon).unwrap()
        };
        let middle = place(f64::from(address.x) + 0.5, f64::from(address.y) + 0.5);
        let mut rgba = Vec::with_capacity((side * side * 4) as usize);
        for row in 0..side {
            for column in 0..side {
                let here = place(
                    f64::from(address.x) + (f64::from(column) + 0.5) / f64::from(side),
                    f64::from(address.y) + (f64::from(row) + 0.5) / f64::from(side),
                );
                let distance = middle.distance_m(here);
                let (east, north) = navigation::bearing_deg(middle, here).to_radians().sin_cos();
                let coded = height(distance * east, distance * north) + 32_768.0;
                let whole = coded.floor();
                rgba.extend([
                    (whole / 256.0).floor() as u8,
                    (whole % 256.0) as u8,
                    ((coded - whole) * 256.0) as u8,
                    255,
                ]);
            }
        }
        let mut terrain = Terrain::new();
        terrain.add_pixels(address, side, &rgba).unwrap();
        (terrain, middle)
    }

    #[test]
    fn from_a_summit_the_glide_follows_the_slope_down_and_ends_on_the_floor() {
        // A cone of 2000 m on a floor of 800 m, falling a metre in two
        // (27°): its foot is 2.4 km out. From its top on the ground with
        // a glide of 8 the slope falls away faster than the glide and the
        // growing reserve, and on the floor 8 × (2000 − 800 − 200) m is
        // reached: 8 km every way.
        let (terrain, summit) = ground(|east, north| (2000.0 - 0.5 * east.hypot(north)).max(800.0));
        let reach = reach(&terrain, summit, 0.0, None, &glider(), 200.0).unwrap();
        assert!((reach.ground_m - 2000.0).abs() < 15.0, "{}", reach.ground_m);
        // A height under the ground is the ground's.
        assert_eq!(reach.altitude_m, reach.ground_m);
        assert_eq!(reach.edges.len(), DIRECTIONS);
        let lose = reach.altitude_m - 800.0 - 200.0;
        for edge in &reach.edges {
            assert_eq!(edge.end, EdgeEnd::Ground, "{edge:?}");
            assert!((edge.distance_m - 8.0 * lose).abs() < 30.0, "{edge:?}");
            assert!((edge.arrival_m - 1000.0).abs() < 5.0, "{edge:?}");
            assert!((edge.ground_m.unwrap() - 800.0).abs() < 1.0, "{edge:?}");
        }

        // From high above it, the ground held ends before the glide does,
        // and the edge says so.
        let high = super::reach(&terrain, summit, 4000.0, None, &glider(), 200.0).unwrap();
        for edge in &high.edges {
            assert_eq!(edge.end, EdgeEnd::Unknown, "{edge:?}");
            assert!(
                edge.distance_m > 12_000.0 && edge.distance_m < 19_000.0,
                "{edge:?}"
            );
            assert!(edge.arrival_m > 1500.0);
        }
        // Where no ground is held there is nothing to say.
        let elsewhere = GeoPoint::new(46.0, 8.0).unwrap();
        assert_eq!(
            super::reach(&terrain, elsewhere, 3000.0, None, &glider(), 200.0),
            None
        );
    }

    #[test]
    fn a_summit_whose_ground_falls_away_slowly_gives_no_way() {
        // A broad dome falling 0.1 m a metre: the glide of 8 and the
        // growing reserve want 0.325, so from its top on the ground the
        // glide ends at once; 300 m above it, it does not.
        let (terrain, summit) = ground(|east, north| (1500.0 - 0.1 * east.hypot(north)).max(800.0));
        let on_top = reach(&terrain, summit, 0.0, None, &glider(), 200.0).unwrap();
        assert!(
            on_top
                .edges
                .iter()
                .all(|edge| edge.end == EdgeEnd::Ground && edge.distance_m < 1.0),
            "{:?}",
            on_top.edges[0]
        );
        let above = reach(&terrain, summit, 1800.0, None, &glider(), 200.0).unwrap();
        assert!(above.edges.iter().all(|edge| edge.distance_m > 3000.0));
    }

    #[test]
    fn in_a_valley_the_glide_goes_far_along_it_and_short_across() {
        // A valley from north to south, its floor at 600 m and 2 km wide,
        // its sides rising one metre in two. From 1500 m in its middle:
        // along it 8 × (1500 − 600 − 200) = 5.6 km; across it the side
        // comes up 1 km out and the glide ends on it,
        // 1500 − d/8 − 200 = 600 + (d − 1000)/2, so d = 1.92 km, where the
        // ground is at 1060 m.
        let (terrain, middle) = ground(|east, _| 600.0 + ((east.abs() - 1000.0).max(0.0) / 2.0));
        let reach = reach(&terrain, middle, 1500.0, None, &glider(), 200.0).unwrap();
        let (north, east, south, west) = (
            &reach.edges[0],
            &reach.edges[18],
            &reach.edges[36],
            &reach.edges[54],
        );
        for along in [north, south] {
            assert!((along.distance_m - 5600.0).abs() < 30.0, "{along:?}");
        }
        for across in [east, west] {
            assert!((across.distance_m - 1920.0).abs() < 30.0, "{across:?}");
            assert_eq!(across.end, EdgeEnd::Ground);
            assert!(
                (across.ground_m.unwrap() - 1060.0).abs() < 15.0,
                "{across:?}"
            );
        }
    }

    #[test]
    fn a_ridge_in_the_way_ends_the_glide_before_it() {
        // Flat at 800 m with a ridge of 1800 m from 2.0 to 2.3 km east.
        // From 2000 m: eastwards the glide is at 1750 m at the ridge, under
        // its top and the reserve, and ends where the ridge begins;
        // westwards it gets 8 × 1000 m. Behind the ridge, low again, it is
        // still out of reach: nothing flies round it.
        let (terrain, start) = ground(|east, _| {
            if (2000.0..=2300.0).contains(&east) {
                1800.0
            } else {
                800.0
            }
        });
        let reach = reach(&terrain, start, 2000.0, None, &glider(), 200.0).unwrap();
        let (east, west) = (&reach.edges[18], &reach.edges[54]);
        // Within a step and a pixel of it: between the places read every
        // 100 m the ground is taken as straight.
        assert!(
            east.distance_m > 1900.0 && east.distance_m <= 2100.0,
            "{east:?}"
        );
        assert_eq!(east.end, EdgeEnd::Ground);
        assert!((west.distance_m - 8000.0).abs() < 30.0, "{west:?}");

        // A west wind of 4 m/s: a glide of 8 × 6/10 westwards, at the
        // same sink.
        let wind = Wind {
            from_deg: 270.0,
            speed_ms: 4.0,
        };
        let windy = super::reach(&terrain, start, 2000.0, Some(wind), &glider(), 200.0).unwrap();
        assert!(
            (windy.edges[54].distance_m - 4.8 * 1000.0).abs() < 30.0,
            "{:?}",
            windy.edges[54]
        );
        assert!((windy.edges[54].arrival_m - 1000.0).abs() < 5.0);
        // And one the glider makes no way against.
        let storm = Wind {
            from_deg: 270.0,
            speed_ms: 12.0,
        };
        let stormy = super::reach(&terrain, start, 2000.0, Some(storm), &glider(), 200.0).unwrap();
        assert_eq!(stormy.edges[54].end, EdgeEnd::NoWay);
        assert_eq!(stormy.edges[54].distance_m, 0.0);
        assert_eq!(stormy.edges[54].arrival_m, 2000.0);
    }

    #[test]
    fn a_glide_ends_on_flat_ground_where_the_reserve_is_left() {
        // Flat ground at 500 m, the glider at 1,600 m: with 100 m to
        // spare a glide of 8 gets 8 km.
        let flat: Vec<Option<f64>> = vec![Some(500.0); 201];
        let (reached, end) = along(&flat, 1600.0, 8.0, 100.0);
        assert!((reached - 8000.0).abs() < 1e-6, "{reached}");
        assert_eq!(end, EdgeEnd::Ground);

        // A slope that rises from 500 m at two kilometres by a metre
        // every two: the glide meets its reserve over it sooner.
        let slope: Vec<Option<f64>> = (0..201)
            .map(|place| Some(500.0 + (f64::from(place) * 100.0 - 2000.0).max(0.0) / 2.0))
            .collect();
        let (reached, end) = along(&slope, 1600.0, 8.0, 100.0);
        // 1,600 − d/8 − 100 = 500 + (d − 2,000)/2, so d = 3,200.
        assert!((reached - 3200.0).abs() < 1e-6, "{reached}");
        assert_eq!(end, EdgeEnd::Ground);

        // Where the ground held ends after three kilometres, the glide is
        // known to get that far and no further is said.
        let short: Vec<Option<f64>> = (0..201)
            .map(|place| (place <= 30).then_some(500.0))
            .collect();
        assert_eq!(
            along(&short, 1600.0, 8.0, 100.0),
            (3000.0, EdgeEnd::Unknown)
        );

        // From very high the glide is followed to the end of the line.
        assert_eq!(
            along(&flat, 5000.0, 8.0, 100.0),
            (20_000.0, EdgeEnd::Unknown)
        );
    }

    #[test]
    fn the_edge_goes_around_the_glider_and_the_wind_shifts_it() {
        // One tile of flat ground at 500 m, sixteen kilometres wide at
        // zoom 11, with the glider in its middle.
        let middle = GeoPoint::new(47.57, 11.69).unwrap();
        let address = TileAddress::holding(middle, 11).unwrap();
        let mut terrain = Terrain::new();
        // 500 m in the Terrarium encoding: 33,268 = 129 × 256 + 244.
        let pixels: Vec<u8> = (0..16 * 16).flat_map(|_| [129, 244, 0, 255]).collect();
        terrain.add_pixels(address, 16, &pixels).unwrap();
        assert_eq!(terrain.elevation_m(middle), Some(500.0));

        // In still air from 1,000 m: 400 m to lose, 3.2 km every way.
        let edge = range(&terrain, middle, 1000.0, None, &glider(), 100.0).unwrap();
        assert_eq!(edge.len(), DIRECTIONS);
        assert_eq!(edge[18].bearing_deg, 90.0);
        for point in &edge {
            assert!((point.distance_m - 3200.0).abs() < 1.0, "{point:?}");
            assert_eq!(point.end, EdgeEnd::Ground);
            assert!((middle.distance_m(point.position) - 3200.0).abs() < 20.0);
        }

        // A west wind of 5 m/s: 15 m/s over the ground eastwards and 5
        // westwards, at the same sink, so a glide of 12 and of 4.
        let wind = Wind {
            from_deg: 270.0,
            speed_ms: 5.0,
        };
        let edge = range(&terrain, middle, 1000.0, Some(wind), &glider(), 100.0).unwrap();
        assert!((edge[18].distance_m - 4800.0).abs() < 1.0, "{:?}", edge[18]);
        assert!((edge[54].distance_m - 1600.0).abs() < 1.0, "{:?}", edge[54]);

        // A wind the glider makes no way against: nothing upwind.
        let storm = Wind {
            from_deg: 270.0,
            speed_ms: 12.0,
        };
        let edge = range(&terrain, middle, 1000.0, Some(storm), &glider(), 100.0).unwrap();
        assert_eq!((edge[54].distance_m, edge[54].end), (0.0, EdgeEnd::NoWay));
        assert!(edge[18].distance_m > 4800.0);

        // Lower than the reserve over the ground, and where no ground is
        // held, there is no edge to draw.
        assert_eq!(range(&terrain, middle, 560.0, None, &glider(), 100.0), None);
        let elsewhere = GeoPoint::new(46.0, 8.0).unwrap();
        assert_eq!(
            range(&terrain, elsewhere, 3000.0, None, &glider(), 100.0),
            None
        );

        // From high up the edge is where the ground held ends, and says so.
        let edge = range(&terrain, middle, 4000.0, None, &glider(), 100.0).unwrap();
        assert!(edge.iter().all(|point| point.end == EdgeEnd::Unknown));
        assert!(edge.iter().all(|point| point.distance_m < 12_000.0));
    }
}
