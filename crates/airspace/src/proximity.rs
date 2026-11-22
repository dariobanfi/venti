//! The airspace that matters next, as a standing reading.
//!
//! The warnings speak when something is urgent. Between them a pilot still
//! wants to read which airspace comes next, how far off its outline is and
//! how it lies in height. [`proximity`] answers that for one position.
//!
//! It is the warnings' own check without their margin, so that the reading
//! and a warning can never contradict each other:
//!
//! - **The same airspaces.** The table that decides what is warned about
//!   ([`Config::policy`]) decides what is read: everything it does not call
//!   free. An airspace no warning could ever be about is never the reading.
//! - **The same two measurements.** The distance and direction to the
//!   outline come from [`horizontal`], the height to floor and ceiling from
//!   [`vertical_status`], which takes what is unknown toward the pilot. For
//!   the same airspace at the same position the reading and the warning
//!   therefore agree in every number, and in what is in doubt.
//! - **The same order of nearness.** See below.
//!
//! # Which one, among several
//!
//! Every airspace that counts and whose outline is within
//! [`Config::proximity_reach_m`] over the ground is a candidate, at
//! whatever height it lies. Of them the first is taken in this order:
//!
//! 1. One the pilot has not set aside before one the pilot has (the
//!    warnings' "this airspace does not apply to me now"). A warning the
//!    pilot only acknowledged changes nothing here: the airspace is where
//!    it was.
//! 2. One to avoid before one that only deserves attention
//!    ([`Policy::Avoid`] before [`Policy::Caution`]). A pilot crossing a
//!    nature reserve for an hour would otherwise read nothing else.
//! 3. The one deeper in the warnings' margins: inside, within the "very
//!    near" margin, within the "near" margin, beyond. Within the same step,
//!    the smaller *warning distance*: the distance over the ground counted
//!    as the margins count it against height (a quarter, by default), or
//!    the height between glider and airspace, whichever is larger. That is
//!    the measure whose steps the margins are: 500 m of it is "near", 100 m
//!    "very near", none "inside". So the reading is the airspace that
//!    raises, or has raised, the most urgent warning its position alone
//!    can give.
//! 4. The nearer over the ground, then the lower identifier, so that the
//!    answer never depends on the order of the set.
//!
//! The order is a pure function of the position: unlike a warning the
//! reading has no memory, does not predict and holds nothing back. It may
//! change its airspace from one position to the next.

use std::cmp::Ordering;

use venti_domain::{Airspace, AirspaceId, GeoPoint};

use crate::index::{AirspaceIndex, Horizontal, horizontal};
use crate::policy::Policy;
use crate::vertical::{Altitudes, VerticalStatus, vertical_status};
use crate::warning::{Config, Margin, within};

/// The airspace a standing reading is about, and where the pilot is in
/// relation to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Proximity<'a> {
    /// The airspace. Its record has the name, the published limits, the
    /// source and the age of the data.
    pub airspace: &'a Airspace,
    /// Whether it is one to avoid or one to be careful in. Never
    /// [`Policy::Free`].
    pub policy: Policy,
    /// Where the pilot is in relation to its outline: inside or not, and
    /// the distance and direction to the nearest edge. From inside that is
    /// the nearest way out.
    pub horizontal: Horizontal,
    /// Where the pilot is in relation to its floor and ceiling, with what
    /// is unknown taken toward the pilot.
    pub vertical: VerticalStatus,
    /// Whether the pilot has set this airspace aside. It is then the
    /// reading only because no other airspace is within reach.
    pub set_aside: bool,
}

impl Proximity<'_> {
    /// Whether the pilot may in fact be clear of the airspace's height
    /// band: see [`VerticalStatus::uncertain`].
    pub fn uncertain(&self) -> bool {
        self.vertical.uncertain
    }
}

/// The airspace that matters next at a position, out to
/// [`Config::proximity_reach_m`] over the ground. `None` when no airspace
/// that counts has its outline that near.
///
/// `set_aside` says which airspaces the pilot has declared not to apply for
/// now; [`WarningEngine::proximity`](crate::WarningEngine::proximity) asks
/// with what the engine was told. `None` says nothing about the air where
/// the set holds no airspace at all: no data is not "no airspace".
///
/// The cost is that of the warnings: only the airspaces the grid lists
/// around the position are looked at, the table sets aside what is free
/// before any geometry, and the distance to an outline is worked out only
/// for an airspace whose height alone does not already put it behind the
/// best found so far.
pub fn proximity<'a>(
    index: &'a AirspaceIndex,
    config: &Config,
    position: GeoPoint,
    altitudes: &Altitudes,
    set_aside: impl Fn(&AirspaceId) -> bool,
) -> Option<Proximity<'a>> {
    let reach_m = if config.proximity_reach_m.is_finite() {
        config.proximity_reach_m.max(0.0)
    } else {
        0.0
    };
    let mut best: Option<(Rank, Proximity<'a>)> = None;
    for airspace in index.near(position, reach_m) {
        let policy = config.policy.policy_for(airspace);
        if policy == Policy::Free {
            continue;
        }
        let set_aside = set_aside(&airspace.id);
        let vertical = vertical_status(airspace, altitudes);
        // With the glider inside its outline an airspace ranks as well as
        // it can. If that is not enough, its outline need not be measured.
        let at_best = Rank::of(config, set_aside, policy, 0.0, &vertical);
        if best
            .as_ref()
            .is_some_and(|(found, _)| at_best.cmp(found) == Ordering::Greater)
        {
            continue;
        }
        let Some(horizontal) = horizontal(airspace, position) else {
            continue;
        };
        let separation_m = horizontal.separation_m();
        if separation_m > reach_m {
            continue;
        }
        let rank = Rank::of(config, set_aside, policy, separation_m, &vertical);
        let better = best.as_ref().is_none_or(|(found, nearest)| {
            rank.cmp(found)
                .then_with(|| airspace.id.cmp(&nearest.airspace.id))
                == Ordering::Less
        });
        if better {
            best = Some((
                rank,
                Proximity {
                    airspace,
                    policy,
                    horizontal,
                    vertical,
                    set_aside,
                },
            ));
        }
    }
    best.map(|(_, nearest)| nearest)
}

/// Where an airspace stands in the order of the module's documentation:
/// the smaller, the more it matters.
#[derive(Debug, Clone, Copy)]
struct Rank {
    set_aside: bool,
    /// False for an airspace to avoid, which comes first.
    caution: bool,
    /// How deep in the warnings' margins: 0 inside, 1 within "very near",
    /// 2 within "near", 3 beyond.
    step: u8,
    /// The warning distance, metres.
    measure_m: f64,
    /// Metres over the ground to the outline, none inside it.
    ground_m: f64,
}

impl Rank {
    fn of(
        config: &Config,
        set_aside: bool,
        policy: Policy,
        ground_m: f64,
        vertical: &VerticalStatus,
    ) -> Self {
        let nothing = Margin {
            horizontal_m: 0.0,
            vertical_m: 0.0,
        };
        // The warnings' own check, so that a step here is a level there.
        let inside = |margin: Margin| within(ground_m, vertical.separation_m, margin, nothing);
        let step = if inside(nothing) {
            0
        } else if inside(config.very_near) {
            1
        } else if inside(config.near) {
            2
        } else {
            3
        };
        // Metres over the ground that count as much as one of height: the
        // shape of the "near" margin, four to one unless it was changed.
        let ground_per_height = config.near.horizontal_m / config.near.vertical_m;
        let ground_per_height = if ground_per_height.is_finite() && ground_per_height > 0.0 {
            ground_per_height
        } else {
            4.0
        };
        Self {
            set_aside,
            caution: policy != Policy::Avoid,
            step,
            measure_m: (ground_m / ground_per_height).max(vertical.separation_m),
            ground_m,
        }
    }

    fn cmp(&self, other: &Self) -> Ordering {
        self.set_aside
            .cmp(&other.set_aside)
            .then(self.caution.cmp(&other.caution))
            .then(self.step.cmp(&other.step))
            .then(self.measure_m.total_cmp(&other.measure_m))
            .then(self.ground_m.total_cmp(&other.ground_m))
    }
}

#[cfg(test)]
mod tests {
    use venti_domain::{
        Activity, AirspaceClass, AirspaceType, HeightUnit, Polygon, Validity, VerticalLimit,
        VerticalReference,
    };

    use super::*;
    use crate::geometry::{M_PER_DEG, destination};
    use crate::vertical::{PressureAltitude, Unknown, VerticalRelation};
    use crate::warning::{Level, Sample, Silence, WarningEngine};

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    fn msl(metres: f64) -> VerticalLimit {
        VerticalLimit::new(VerticalReference::MeanSeaLevel, metres, HeightUnit::Metres).unwrap()
    }

    /// A rectangle between two corners.
    fn airspace(
        id: &str,
        kind: (AirspaceClass, AirspaceType),
        south_west: (f64, f64),
        north_east: (f64, f64),
        limits: (VerticalLimit, VerticalLimit),
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
            class: kind.0,
            airspace_type: kind.1,
            country: None,
            lower: limits.0,
            upper: limits.1,
            lower_at_least: None,
            upper_at_most: None,
            geometry: Polygon::new(ring, Vec::new()).unwrap(),
            activity: Activity::default(),
            frequencies: Vec::new(),
            remarks: None,
            validity: Validity::default(),
        }
    }

    const CTR: (AirspaceClass, AirspaceType) = (AirspaceClass::D, AirspaceType::Ctr);
    const TMA: (AirspaceClass, AirspaceType) = (AirspaceClass::C, AirspaceType::Tma);

    /// A control zone from the ground to 1500 m, and a terminal area over
    /// it and around it from 1500 m to 3000 m.
    fn innsbruck() -> Vec<Airspace> {
        vec![
            airspace(
                "ctr",
                CTR,
                (47.2, 11.2),
                (47.3, 11.5),
                (VerticalLimit::ground(), msl(1500.0)),
            ),
            airspace(
                "tma",
                TMA,
                (47.1, 11.0),
                (47.4, 11.7),
                (msl(1500.0), msl(3000.0)),
            ),
        ]
    }

    /// A place a distance north of the terminal area's northern edge.
    fn north_of_the_area(metres: f64) -> GeoPoint {
        destination(point(47.4, 11.35), 0.0, metres).unwrap()
    }

    fn read<'a>(
        index: &'a AirspaceIndex,
        position: GeoPoint,
        altitude_m: f64,
    ) -> Option<Proximity<'a>> {
        proximity(
            index,
            &Config::default(),
            position,
            &Altitudes::gps(altitude_m),
            |_| false,
        )
    }

    #[test]
    fn beside_an_airspace_at_its_height_the_reading_says_how_far_and_which_way() {
        let index = AirspaceIndex::new(innsbruck());
        // 3.2 km north of the terminal area, at 2000 m: between its floor
        // and its ceiling.
        let found = read(&index, north_of_the_area(3200.0), 2000.0).unwrap();
        assert_eq!(found.airspace.name, "TMA");
        assert_eq!(found.policy, Policy::Avoid);
        assert!(!found.horizontal.inside);
        assert!((found.horizontal.distance_m - 3200.0).abs() < 1.0);
        assert!((found.horizontal.bearing_deg - 180.0).abs() < 0.01);
        assert_eq!(found.vertical.relation, VerticalRelation::Inside);
        assert_eq!(found.vertical.separation_m, 0.0);
        assert!(!found.uncertain() && !found.set_aside);
    }

    #[test]
    fn under_a_floor_and_over_a_ceiling_the_reading_says_by_how_much() {
        let index = AirspaceIndex::new(innsbruck());
        // The same place at 1100 m: the floor is 400 m above.
        let found = read(&index, north_of_the_area(3200.0), 1100.0).unwrap();
        assert_eq!(found.airspace.name, "TMA");
        assert_eq!(found.vertical.relation, VerticalRelation::Below);
        assert!((found.vertical.separation_m - 400.0).abs() < 1e-9);
        assert!((found.horizontal.distance_m - 3200.0).abs() < 1.0);

        // And at 3250 m: the ceiling is 250 m below.
        let found = read(&index, north_of_the_area(3200.0), 3250.0).unwrap();
        assert_eq!(found.airspace.name, "TMA");
        assert_eq!(found.vertical.relation, VerticalRelation::Above);
        assert!((found.vertical.separation_m - 250.0).abs() < 1e-9);
    }

    #[test]
    fn inside_an_outline_but_under_the_floor_the_airspace_is_overhead() {
        let index = AirspaceIndex::new(innsbruck());
        // West of the control zone, under the terminal area, at 1176 m:
        // 324 m under its floor. The zone is 3.8 km to the east, at the
        // glider's height, and still the area overhead matters first: a
        // warning for it is 324 m of climb away, one for the zone 1.8 km
        // of flying.
        let here = point(47.25, 11.15);
        let found = read(&index, here, 1176.0).unwrap();
        assert_eq!(found.airspace.name, "TMA");
        assert!(found.horizontal.inside);
        assert_eq!(found.horizontal.separation_m(), 0.0);
        assert_eq!(found.vertical.relation, VerticalRelation::Below);
        assert!((found.vertical.separation_m - 324.0).abs() < 1e-9);
        // From inside the outline the distance is to the nearest way out.
        // The western edge, some eleven kilometres off.
        assert!(
            (11_000.0..11_700.0).contains(&found.horizontal.distance_m),
            "{}",
            found.horizontal.distance_m
        );
        assert!((found.horizontal.bearing_deg - 270.0).abs() < 0.5);

        // Lower down, at 400 m, the floor is 1100 m above: more than the
        // zone's 3.8 km count for, a quarter of them. Now the zone is next.
        let found = read(&index, here, 400.0).unwrap();
        assert_eq!(found.airspace.name, "CTR");
        assert!(!found.horizontal.inside);
        assert!((found.horizontal.bearing_deg - 90.0).abs() < 0.5);
        assert_eq!(found.vertical.relation, VerticalRelation::Inside);
    }

    #[test]
    fn of_two_candidates_the_one_nearer_to_a_warning_is_read() {
        // Two zones at the glider's height, one 1 km to the west and one
        // 3 km to the east.
        let here = point(47.0, 11.0);
        let degree = M_PER_DEG * 47.0_f64.to_radians().cos();
        let (near_edge, far_edge) = (11.0 - 1000.0 / degree, 11.0 + 3000.0 / degree);
        let limits = (VerticalLimit::ground(), msl(3000.0));
        let west = airspace(
            "west",
            CTR,
            (46.9, near_edge - 0.1),
            (47.1, near_edge),
            limits,
        );
        let east = airspace(
            "east",
            CTR,
            (46.9, far_edge),
            (47.1, far_edge + 0.1),
            limits,
        );
        // The order of the set does not decide.
        for set in [
            vec![west.clone(), east.clone()],
            vec![east.clone(), west.clone()],
        ] {
            let index = AirspaceIndex::new(set);
            assert_eq!(read(&index, here, 1500.0).unwrap().airspace.name, "WEST");
        }

        // Height counts four times as much as distance over the ground, as
        // in the warnings' margins. The western zone now begins 500 m above
        // the glider: that is as far from a warning as 2 km over the ground,
        // and the eastern one, 3 km off at the glider's height, is further.
        let high_west = airspace(
            "west",
            CTR,
            (46.9, near_edge - 0.1),
            (47.1, near_edge),
            (msl(2000.0), msl(3000.0)),
        );
        let index = AirspaceIndex::new(vec![high_west.clone(), east.clone()]);
        assert_eq!(read(&index, here, 1500.0).unwrap().airspace.name, "WEST");
        // 900 m above, it is further than the eastern one.
        assert_eq!(read(&index, here, 1100.0).unwrap().airspace.name, "EAST");

        // Two that are equally near are told apart by their identifiers.
        let twin = airspace(
            "another",
            CTR,
            (46.9, near_edge - 0.1),
            (47.1, near_edge),
            limits,
        );
        for set in [vec![west.clone(), twin.clone()], vec![twin, west]] {
            let index = AirspaceIndex::new(set);
            assert_eq!(read(&index, here, 1500.0).unwrap().airspace.name, "ANOTHER");
        }
    }

    #[test]
    fn nothing_within_reach_is_no_reading() {
        let index = AirspaceIndex::new(innsbruck());
        // 4.9 km north of the terminal area there is a reading, at any
        // height: the outline is what must be within reach.
        for altitude in [300.0, 2000.0, 9000.0] {
            let found = read(&index, north_of_the_area(4900.0), altitude).unwrap();
            assert_eq!(found.airspace.name, "TMA");
        }
        // 5.1 km north there is none.
        assert_eq!(read(&index, north_of_the_area(5100.0), 2000.0), None);
        // Nor where the set holds nothing, or is empty.
        assert_eq!(read(&index, point(10.0, 100.0), 2000.0), None);
        let empty = AirspaceIndex::new(Vec::new());
        assert_eq!(read(&empty, point(47.25, 11.35), 2000.0), None);

        // The reach is a setting. One that is no distance reads only what
        // the glider is in the outline of.
        let mut config = Config::default();
        for reach in [f64::NAN, -1.0, 0.0] {
            config.proximity_reach_m = reach;
            let read = |position| {
                proximity(&index, &config, position, &Altitudes::gps(2000.0), |_| {
                    false
                })
            };
            assert_eq!(read(north_of_the_area(100.0)), None);
            assert_eq!(read(point(47.25, 11.15)).unwrap().airspace.name, "TMA");
        }
    }

    #[test]
    fn an_airspace_that_does_not_count_is_never_the_reading() {
        // Class E all around, a glider sector, and far to the east a
        // control zone: the glider is inside the first two, which a
        // paraglider needs nothing for.
        let everywhere = ((46.5, 10.0), (48.0, 12.5));
        let echo = airspace(
            "echo",
            (AirspaceClass::E, AirspaceType::Unspecified),
            everywhere.0,
            everywhere.1,
            (VerticalLimit::ground(), msl(3000.0)),
        );
        let sector = airspace(
            "sector",
            (AirspaceClass::Unclassified, AirspaceType::GliderSector),
            (47.1, 11.0),
            (47.4, 11.7),
            (VerticalLimit::ground(), msl(3000.0)),
        );
        let index = AirspaceIndex::new(vec![echo.clone(), sector.clone()]);
        assert_eq!(read(&index, point(47.25, 11.35), 1500.0), None);

        // With a zone 4 km off, the zone is read, not the air around it.
        let here = point(47.25, 11.35);
        let degree = M_PER_DEG * 47.25_f64.to_radians().cos();
        let edge = 11.35 + 4000.0 / degree;
        let zone = airspace(
            "ctr",
            CTR,
            (47.2, edge),
            (47.3, edge + 0.1),
            (VerticalLimit::ground(), msl(3000.0)),
        );
        let index = AirspaceIndex::new(vec![echo, sector, zone]);
        let found = read(&index, here, 1500.0).unwrap();
        assert_eq!(found.airspace.name, "CTR");
        assert!((found.horizontal.distance_m - 4000.0).abs() < 5.0);
    }

    #[test]
    fn an_airspace_to_avoid_is_read_before_one_that_deserves_attention() {
        // The glider is inside a nature reserve, with a control zone 3 km
        // to the east.
        let here = point(47.25, 11.35);
        let degree = M_PER_DEG * 47.25_f64.to_radians().cos();
        let edge = 11.35 + 3000.0 / degree;
        let reserve = airspace(
            "reserve",
            (AirspaceClass::Unclassified, AirspaceType::Protected),
            (47.0, 11.0),
            (47.5, 11.7),
            (VerticalLimit::ground(), msl(3000.0)),
        );
        let zone = airspace(
            "ctr",
            CTR,
            (47.2, edge),
            (47.3, edge + 0.1),
            (VerticalLimit::ground(), msl(3000.0)),
        );
        let index = AirspaceIndex::new(vec![reserve.clone(), zone]);
        assert_eq!(read(&index, here, 1500.0).unwrap().airspace.name, "CTR");

        // Alone, the reserve is the reading: the warnings speak of it too,
        // and a reading must not be empty while they do.
        let index = AirspaceIndex::new(vec![reserve]);
        let found = read(&index, here, 1500.0).unwrap();
        assert_eq!(found.airspace.name, "RESERVE");
        assert_eq!(found.policy, Policy::Caution);
        assert!(found.horizontal.inside);
    }

    #[test]
    fn what_is_not_known_is_taken_toward_the_pilot_as_in_the_warnings() {
        // A restricted area from 300 m above ground, 2 km to the north.
        // Without the ground's height its floor may be anywhere under the
        // glider: "at your height, possibly", and the reading says what is
        // missing.
        let here = point(47.0, 11.0);
        let edge = 47.0 + 2000.0 / M_PER_DEG;
        let above_ground =
            VerticalLimit::new(VerticalReference::AboveGround, 300.0, HeightUnit::Metres).unwrap();
        let area = airspace(
            "ed-r",
            (AirspaceClass::Unclassified, AirspaceType::Restricted),
            (edge, 10.9),
            (edge + 0.1, 11.1),
            (above_ground, VerticalLimit::flight_level(100).unwrap()),
        );
        let index = AirspaceIndex::new(vec![area]);
        let config = Config::default();
        let found = proximity(&index, &config, here, &Altitudes::gps(900.0), |_| false).unwrap();
        assert_eq!(found.vertical.relation, VerticalRelation::Inside);
        assert!(found.uncertain());
        assert_eq!(found.vertical.missing(), Some(Unknown::GroundElevation));

        // With the ground at 700 m the floor is at 1000 m, 100 m above.
        let known = Altitudes {
            altitude_m: 900.0,
            qnh_altitude_m: None,
            pressure: Some(PressureAltitude::standard(900.0)),
            ground_elevation_m: Some(700.0),
        };
        let found = proximity(&index, &config, here, &known, |_| false).unwrap();
        assert_eq!(found.vertical.relation, VerticalRelation::Below);
        assert!((found.vertical.separation_m - 100.0).abs() < 1e-9);
        assert!(!found.uncertain());
    }

    /// A sample on a standard day, standing still.
    fn sample(time_s: f64, position: GeoPoint, altitude_m: f64) -> Sample {
        Sample {
            time_s,
            position,
            altitude_m,
            qnh_altitude_m: None,
            pressure: Some(PressureAltitude::standard(altitude_m)),
            ground_elevation_m: Some(600.0),
            ground_speed_ms: None,
            course_deg: None,
            climb_ms: None,
        }
    }

    #[test]
    fn the_reading_and_the_warnings_agree_wherever_the_glider_is() {
        let index = AirspaceIndex::new(innsbruck());
        // A grid of places in and around the two airspaces, at heights
        // under, in and over them. Each is looked at by an engine of its
        // own, so that no warning is held over from the place before.
        let mut warned = 0;
        for row in 0..=24 {
            for column in 0..=24 {
                for altitude in [700.0, 1050.0, 1420.0, 1480.0, 1600.0, 2950.0, 3300.0] {
                    let position = point(
                        47.0 + 0.5 * f64::from(row) / 24.0,
                        10.9 + 0.9 * f64::from(column) / 24.0,
                    );
                    let here = sample(0.0, position, altitude);
                    let mut engine = WarningEngine::new(Config::default());
                    let warnings = engine.update(&index, &here).unwrap();
                    let reading = engine.proximity(&index, &here).unwrap();
                    let Some(most_urgent) = warnings.first() else {
                        continue;
                    };
                    warned += 1;
                    // While a warning is up there is a reading, and it is
                    // about an airspace that is warned of at least as
                    // urgently as any other.
                    let reading = reading.expect("a warning without a reading");
                    let of_the_reading = warnings
                        .iter()
                        .find(|warning| warning.id() == &reading.airspace.id)
                        .expect("the reading's airspace has no warning");
                    assert_eq!(of_the_reading.level, most_urgent.level);
                    // And the two say the same of it.
                    assert_eq!(of_the_reading.horizontal, reading.horizontal);
                    assert_eq!(of_the_reading.vertical, reading.vertical);
                    assert_eq!(of_the_reading.policy, reading.policy);
                }
            }
        }
        assert!(warned > 1000, "{warned}");
    }

    #[test]
    fn an_airspace_the_pilot_set_aside_is_read_only_when_no_other_is_within_reach() {
        let index = AirspaceIndex::new(innsbruck());
        let zone: AirspaceId = "test:ctr".parse().unwrap();
        let area: AirspaceId = "test:tma".parse().unwrap();
        // In the control zone at 1200 m, 300 m under the terminal area.
        let here = sample(0.0, point(47.25, 11.35), 1200.0);
        let mut engine = WarningEngine::new(Config::default());
        let warnings = engine.update(&index, &here).unwrap();
        assert_eq!(warnings[0].level, Level::Inside);
        let reading = engine.proximity(&index, &here).unwrap().unwrap();
        assert_eq!(reading.airspace.id, zone);

        // Acknowledged, the zone is still where it is, and still read.
        engine.acknowledge(&zone, Silence::For { seconds: 600.0 });
        let reading = engine.proximity(&index, &here).unwrap().unwrap();
        assert_eq!((&reading.airspace.id, reading.set_aside), (&zone, false));

        // "Does not apply today": the area overhead is next.
        engine.ignore(&zone, Silence::For { seconds: 600.0 });
        let reading = engine.proximity(&index, &here).unwrap().unwrap();
        assert_eq!((&reading.airspace.id, reading.set_aside), (&area, false));
        assert_eq!(reading.vertical.relation, VerticalRelation::Below);

        // Both set aside: the reading is not empty, it is the nearer of
        // the two, and says that it was set aside.
        engine.ignore(&area, Silence::For { seconds: 600.0 });
        let reading = engine.proximity(&index, &here).unwrap().unwrap();
        assert_eq!((&reading.airspace.id, reading.set_aside), (&zone, true));

        // When the time is up the zone counts again; taking it back does
        // the same at once.
        let later = sample(601.0, here.position, 1200.0);
        let reading = engine.proximity(&index, &later).unwrap().unwrap();
        assert_eq!((&reading.airspace.id, reading.set_aside), (&zone, false));
        engine.unsilence(&zone);
        let reading = engine.proximity(&index, &here).unwrap().unwrap();
        assert_eq!((&reading.airspace.id, reading.set_aside), (&zone, false));

        // Set aside on the ground, before the first position.
        let mut engine = WarningEngine::new(Config::default());
        engine.ignore(&zone, Silence::RestOfDay { utc_offset_s: 3600 });
        let reading = engine.proximity(&index, &here).unwrap().unwrap();
        assert_eq!(reading.airspace.id, area);

        // A sample the warnings refuse is refused here too.
        let mut broken = here;
        broken.altitude_m = f64::NAN;
        assert!(engine.proximity(&index, &broken).is_err());
        broken = here;
        broken.time_s = f64::INFINITY;
        assert!(engine.proximity(&index, &broken).is_err());
    }
}
