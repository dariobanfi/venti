//! The nearest landing within glide: of the landing fields the cockpit
//! knows, the nearest one the glider is expected to reach with height to
//! spare, in the wind as last worked out and over the ground as it is.
//!
//! # The rule
//!
//! The fields are looked at nearest first, out to [`LOOK_M`]. A field is
//! within glide when both hold:
//!
//! - the straight glide to it, in the wind, arrives at least the reserve
//!   above the field's height (the caller's, [`RESERVE_M`] in the
//!   cockpit);
//! - where terrain is held, that straight line stays above the ground on
//!   the way, read every [`GROUND_STEP_M`], up to [`FINAL_M`] before the
//!   field.
//!
//! The first field that is, is the answer. When none is, the answer says
//! so and names the field that misses by least.
//!
//! # Its limits
//!
//! The air neither rises nor sinks on the way and the wind is one wind;
//! in the lee of a ridge neither holds. The line is straight: a field
//! behind a ridge is out of reach here although a pilot may fly around
//! it. A field is a point with a height, not a judgement that it can be
//! landed on today. And a field the cockpit was not given is never the
//! answer: "none within glide" means none of those known.

use venti_domain::GeoPoint;

use super::Wind;
use super::navigation::{self, Glides, Target};
use crate::terrain::Terrain;

/// The height to arrive with over a field for it to count as within
/// glide, metres: room for a circuit and for the prediction being a
/// little wrong. The cockpit's reserve for every place; to verify in the
/// air.
pub const RESERVE_M: f64 = 100.0;

/// Fields further than this are not looked at, metres.
pub const LOOK_M: f64 = 30_000.0;

/// The ground under a glide to a field is read this far apart, metres.
pub const GROUND_STEP_M: f64 = 100.0;

/// The last stretch before a field is not held against the ground,
/// metres: there the line comes down to the field by design.
pub const FINAL_M: f64 = 500.0;

/// How one field lies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldReach {
    /// Which of the fields given it is.
    pub index: usize,
    /// How far it is, metres.
    pub distance_m: f64,
    /// The direction to it, degrees clockwise from north.
    pub bearing_deg: f64,
    /// How far above the field the glide is expected to end, metres;
    /// negative when it comes down before it. `None` when the wind lets
    /// the glider make no way towards it.
    pub arrival_height_m: Option<f64>,
    /// Where the straight glide meets the ground before the field, metres
    /// from the glider.
    pub ground_in_the_way_m: Option<f64>,
}

/// The answer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NearestLanding {
    /// The nearest field within glide.
    Within(FieldReach),
    /// No field is within glide; the one that misses by least, if any
    /// has a height and lies within [`LOOK_M`].
    NoneWithin(Option<FieldReach>),
}

/// The nearest landing within glide among `fields`, by the module's rule.
/// A field without a height takes the terrain's at its place, and is left
/// out where that is not held either. `None` when no fields are given.
pub fn nearest_landing(
    terrain: Option<&Terrain>,
    position: GeoPoint,
    altitude_m: f64,
    wind: Option<Wind>,
    glider: &impl Glides,
    fields: &[Target],
    reserve_m: f64,
) -> Option<NearestLanding> {
    if fields.is_empty() {
        return None;
    }
    let mut near: Vec<(usize, f64)> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| (index, position.distance_m(field.position)))
        .filter(|(_, distance)| *distance <= LOOK_M)
        .collect();
    near.sort_by(|a, b| a.1.total_cmp(&b.1));

    let mut least_short: Option<(f64, FieldReach)> = None;
    for (index, _) in near {
        let field = &fields[index];
        let Some(elevation_m) = field
            .elevation_m
            .filter(|elevation| elevation.is_finite())
            .or_else(|| terrain.and_then(|terrain| terrain.elevation_m(field.position)))
        else {
            continue;
        };
        let way = navigation::navigate(
            position,
            Some(altitude_m),
            wind,
            &Target {
                name: String::new(),
                position: field.position,
                elevation_m: Some(elevation_m),
            },
            glider,
        );
        let ground_in_the_way_m = way.arrival_height_m.and_then(|arrival| {
            ground_in_the_way(terrain?, position, altitude_m, &way, elevation_m + arrival)
        });
        let reach = FieldReach {
            index,
            distance_m: way.distance_m,
            bearing_deg: way.bearing_deg,
            arrival_height_m: way.arrival_height_m,
            ground_in_the_way_m,
        };
        let within = ground_in_the_way_m.is_none()
            && way
                .arrival_height_m
                .is_some_and(|arrival| arrival >= reserve_m);
        if within {
            return Some(NearestLanding::Within(reach));
        }
        // By how much it misses: the height short of the reserve, and a
        // ridge in the way or no way at all counted as worse than any
        // height.
        let short = match (way.arrival_height_m, ground_in_the_way_m) {
            (Some(arrival), None) => reserve_m - arrival,
            (Some(arrival), Some(_)) => 1e6 + reserve_m - arrival,
            (None, _) => 1e9,
        };
        if least_short.is_none_or(|(least, _)| short < least) {
            least_short = Some((short, reach));
        }
    }
    Some(NearestLanding::NoneWithin(
        least_short.map(|(_, reach)| reach),
    ))
}

/// Where the straight line from the glider down to `end_m` over the
/// field meets the ground before it gets there, metres from the glider.
fn ground_in_the_way(
    terrain: &Terrain,
    position: GeoPoint,
    altitude_m: f64,
    way: &navigation::Navigation,
    end_m: f64,
) -> Option<f64> {
    let places = ((way.distance_m - FINAL_M) / GROUND_STEP_M).floor();
    if places < 1.0 || end_m > altitude_m {
        return None;
    }
    terrain
        .along(position, way.bearing_deg, GROUND_STEP_M)
        .take(places as usize + 1)
        .enumerate()
        // The glider's own place says nothing: on a slope its height over
        // the ground is the pilot's business.
        .skip(1)
        .find_map(|(index, ground)| {
            let along = index as f64 * GROUND_STEP_M;
            let line = altitude_m - (altitude_m - end_m) * along / way.distance_m;
            ground.filter(|ground| line < *ground).map(|_| along)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geodesy::Earth;
    use crate::live::navigation::Glider;

    fn here() -> GeoPoint {
        GeoPoint::new(47.0, 11.0).unwrap()
    }

    fn field(name: &str, bearing_deg: f64, distance_m: f64, elevation_m: Option<f64>) -> Target {
        Target {
            name: name.to_owned(),
            position: Earth::Wgs84.destination(here(), bearing_deg, distance_m),
            elevation_m,
        }
    }

    const GLIDER: Glider = Glider {
        trim_speed_ms: 10.0,
        glide_ratio: 8.0,
    };

    #[test]
    fn the_nearest_field_within_glide_is_the_answer_not_the_one_arrived_highest_over() {
        // 1,400 m up over fields at 600 m: 800 m to spend, 6.4 km of
        // glide, 5.6 km with the reserve kept.
        let fields = [
            field("Far", 90.0, 9_000.0, Some(600.0)),
            field("Near", 180.0, 2_000.0, Some(700.0)),
            field("Middle", 0.0, 4_000.0, Some(600.0)),
        ];
        let found = nearest_landing(None, here(), 1_400.0, None, &GLIDER, &fields, RESERVE_M);
        let Some(NearestLanding::Within(reach)) = found else {
            panic!("{found:?}");
        };
        assert_eq!(reach.index, 1);
        assert!((reach.distance_m - 2_000.0).abs() < 5.0, "{reach:?}");
        // 700 m over it, less 250 m of glide.
        assert!(
            (reach.arrival_height_m.unwrap() - 450.0).abs() < 2.0,
            "{reach:?}"
        );
        assert!((reach.bearing_deg - 180.0).abs() < 0.5);
    }

    #[test]
    fn a_head_wind_puts_the_nearest_field_out_of_glide_and_the_next_is_taken() {
        let fields = [
            field("Upwind", 0.0, 3_000.0, Some(600.0)),
            field("Downwind", 180.0, 4_000.0, Some(600.0)),
        ];
        // 1,100 m: 500 m to spend. Still air reaches the upwind field
        // with 125 m.
        let still = nearest_landing(None, here(), 1_100.0, None, &GLIDER, &fields, RESERVE_M);
        assert!(matches!(still, Some(NearestLanding::Within(reach)) if reach.index == 0));
        // A northerly of 6 m/s halves the glide into it and makes the
        // glide with it 12.8.
        let northerly = Some(Wind {
            from_deg: 0.0,
            speed_ms: 6.0,
        });
        let windy = nearest_landing(
            None,
            here(),
            1_100.0,
            northerly,
            &GLIDER,
            &fields,
            RESERVE_M,
        );
        let Some(NearestLanding::Within(reach)) = windy else {
            panic!("{windy:?}");
        };
        assert_eq!(reach.index, 1);
        assert!(
            (reach.arrival_height_m.unwrap() - 187.5).abs() < 2.0,
            "{reach:?}"
        );
    }

    #[test]
    fn none_within_glide_is_said_with_the_field_that_misses_by_least() {
        let fields = [
            field("Close but high", 0.0, 3_000.0, Some(900.0)),
            field("Further", 90.0, 5_000.0, Some(600.0)),
        ];
        // 1,000 m: the first is reached 275 m under its height, the
        // second 225 m under.
        let found = nearest_landing(None, here(), 1_000.0, None, &GLIDER, &fields, RESERVE_M);
        let Some(NearestLanding::NoneWithin(Some(reach))) = found else {
            panic!("{found:?}");
        };
        assert_eq!(reach.index, 1);
        assert!(
            (reach.arrival_height_m.unwrap() + 225.0).abs() < 2.0,
            "{reach:?}"
        );

        // No fields: nothing to say. Fields without a height and no
        // terrain: none within glide, and none to name.
        assert_eq!(
            nearest_landing(None, here(), 1_000.0, None, &GLIDER, &[], RESERVE_M),
            None
        );
        let unknown = [field("No height", 0.0, 1_000.0, None)];
        assert_eq!(
            nearest_landing(None, here(), 1_000.0, None, &GLIDER, &unknown, RESERVE_M),
            Some(NearestLanding::NoneWithin(None))
        );
    }

    #[test]
    fn a_ridge_in_the_way_puts_a_field_out_of_glide() {
        // A floor at 600 m with a ridge of 1,300 m lying east to west two
        // kilometres north of the glider.
        let (terrain, middle) = crate::live::range::tests::ground(|_, north| {
            if (1_900.0..2_300.0).contains(&north) {
                1_300.0
            } else {
                600.0
            }
        });
        let at = |bearing_deg: f64, distance_m: f64| Target {
            name: String::new(),
            position: Earth::Wgs84.destination(middle, bearing_deg, distance_m),
            // The field's height is the terrain's.
            elevation_m: None,
        };
        let fields = [at(0.0, 4_000.0), at(180.0, 5_000.0)];
        // 1,400 m: the northern field would be reached 300 m over it, but
        // the line is at 1,150 m where the ridge stands 1,300 m high.
        let found = nearest_landing(
            Some(&terrain),
            middle,
            1_400.0,
            None,
            &GLIDER,
            &fields,
            RESERVE_M,
        );
        let Some(NearestLanding::Within(reach)) = found else {
            panic!("{found:?}");
        };
        assert_eq!(reach.index, 1);
        assert!(
            (reach.arrival_height_m.unwrap() - 175.0).abs() < 5.0,
            "{reach:?}"
        );

        // With only the northern field there is none within glide, and it
        // is named with the ridge before it.
        let found = nearest_landing(
            Some(&terrain),
            middle,
            1_400.0,
            None,
            &GLIDER,
            &fields[..1],
            RESERVE_M,
        );
        let Some(NearestLanding::NoneWithin(Some(reach))) = found else {
            panic!("{found:?}");
        };
        let ridge = reach.ground_in_the_way_m.unwrap();
        assert!((1_800.0..=2_000.0).contains(&ridge), "{reach:?}");
    }
}
