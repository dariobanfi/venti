//! How far the glider glides: the edge of what a straight glide reaches
//! over the ground as it is, in every direction.
//!
//! In each direction the glider's own glide, in the wind as last worked
//! out, is laid over the ground along that line. The edge is where it
//! comes down to a reserve over the ground: on the valley floor after a
//! long glide, or on a slope after a short one. Nothing here flies around
//! a ridge: what lies behind one counts as out of reach, although a pilot
//! may reach it by another way.

use venti_domain::GeoPoint;

use super::navigation::{self, Glider};
use super::wind::Wind;
use crate::geodesy::Earth;
use crate::terrain::Terrain;

/// The edge is worked out in this many directions: every five degrees.
pub const DIRECTIONS: usize = 72;

/// The ground along each line is read this far apart, metres.
const STEP_M: f64 = 100.0;

/// No glide is followed further than this, metres. The ground that is
/// held reaches some thirteen kilometres from the glider.
const AT_MOST_M: f64 = 20_000.0;

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
    let places = (AT_MOST_M / STEP_M) as usize + 1;
    Some(
        (0..DIRECTIONS)
            .map(|direction| {
                let bearing_deg = 360.0 * direction as f64 / DIRECTIONS as f64;
                let (distance_m, end) =
                    match navigation::glide_over_ground(bearing_deg, wind, glider) {
                        None => (0.0, EdgeEnd::NoWay),
                        Some(glide) => {
                            let ground = terrain.profile(position, bearing_deg, STEP_M, places);
                            along(&ground, altitude_m, glide, reserve_m)
                        }
                    };
                Edge {
                    bearing_deg,
                    distance_m,
                    position: Earth::Wgs84.destination(position, bearing_deg, distance_m),
                    end,
                }
            })
            .collect(),
    )
}

/// How far a glide gets over the ground along one line, and why it ends.
/// `ground` holds the ground every [`STEP_M`] from the glider on.
fn along(ground: &[Option<f64>], altitude_m: f64, glide: f64, reserve_m: f64) -> (f64, EdgeEnd) {
    // How far over the reserve the line is at each place.
    let over =
        |place: usize, ground: f64| altitude_m - place as f64 * STEP_M / glide - ground - reserve_m;
    let mut before: Option<f64> = None;
    for (place, height) in ground.iter().enumerate() {
        let Some(height) = height else {
            // The ground held ends here: the glide got as far as the
            // place before.
            return (place.saturating_sub(1) as f64 * STEP_M, EdgeEnd::Unknown);
        };
        let clear = over(place, *height);
        if clear < 0.0 {
            // Between two places the line and the ground are taken as
            // straight.
            let reached = match before {
                Some(before) if before > 0.0 => {
                    (place - 1) as f64 * STEP_M + STEP_M * before / (before - clear)
                }
                _ => place.saturating_sub(1) as f64 * STEP_M,
            };
            return (reached, EdgeEnd::Ground);
        }
        before = Some(clear);
    }
    (
        ground.len().saturating_sub(1) as f64 * STEP_M,
        EdgeEnd::Unknown,
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
