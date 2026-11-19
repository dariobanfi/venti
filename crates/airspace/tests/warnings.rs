//! Flies scripted flights through the warning engine and checks what the
//! pilot would be told, second by second.
//!
//! The airspace is a simplified Innsbruck: a control zone from the ground
//! to 5000 ft, a class C terminal area above and around it from 5000 ft to
//! FL 100, class C above FL 100 everywhere, a class E area, a glider sector
//! and a parachute drop zone. All of it is written as an OpenAir file, the
//! way a pilot's own airspace reaches the engine.

use venti_airspace::openair::{self, Options};
use venti_airspace::{
    AirspaceIndex, Config, EntryVia, Level, Policy, PressureAltitude, Sample, SampleError, Silence,
    Silenced, Warning, WarningEngine, destination,
};
use venti_domain::{AirspaceId, GeoPoint, SourceId};

/// One rectangular airspace as OpenAir text.
fn block(class: &str, kind: &str, name: &str, limits: (&str, &str), corners: [f64; 4]) -> String {
    let [south, west, north, east] = corners;
    format!(
        "AC {class}\nAY {kind}\nAN {name}\nAL {}\nAH {}\n\
         DP {south}N {west}E\nDP {south}N {east}E\nDP {north}N {east}E\nDP {north}N {west}E\n",
        limits.0, limits.1
    )
}

fn airspace() -> AirspaceIndex {
    let text = [
        block(
            "D",
            "CTR",
            "CTR",
            ("GND", "5000ft MSL"),
            [47.2, 11.2, 47.3, 11.5],
        ),
        block(
            "C",
            "TMA",
            "TMA",
            ("5000ft MSL", "FL100"),
            [47.1, 11.0, 47.4, 11.7],
        ),
        block(
            "C",
            "NONE",
            "UPPER",
            ("FL100", "UNL"),
            [46.5, 10.0, 48.0, 12.5],
        ),
        block(
            "E",
            "NONE",
            "ECHO",
            ("1000ft AGL", "FL100"),
            [46.5, 10.0, 48.0, 12.5],
        ),
        block(
            "UNC",
            "GSEC",
            "SECTOR",
            ("5000ft MSL", "7500ft MSL"),
            [47.1, 11.0, 47.2, 11.2],
        ),
        block(
            "UNC",
            "ASRA",
            "DROP ZONE",
            ("GND", "FL100"),
            [47.6, 11.2, 47.65, 11.3],
        ),
    ]
    .concat();
    let file = openair::parse(&text, &Options::new(SourceId::new("test").unwrap()));
    assert!(file.problems.is_empty(), "{:?}", file.problems);
    AirspaceIndex::new(file.airspaces)
}

fn id(name: &str) -> AirspaceId {
    let local = match name {
        "CTR" => "ctr_gnd_5000ft-msl",
        "TMA" => "tma_5000ft-msl_fl100",
        "UPPER" => "upper_fl100_unl",
        "DROP ZONE" => "drop-zone_gnd_fl100",
        other => panic!("no identifier for {other}"),
    };
    format!("test:{local}").parse().unwrap()
}

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).unwrap()
}

/// A sample on a standard day: the pressure altitude equals the altitude.
fn sample(time_s: f64, position: GeoPoint, altitude_m: f64) -> Sample {
    Sample {
        time_s,
        position,
        altitude_m,
        pressure: Some(PressureAltitude::standard(altitude_m)),
        ground_elevation_m: Some(600.0),
        ground_speed_ms: None,
        course_deg: None,
        climb_ms: None,
    }
}

fn level_of(warnings: &[Warning<'_>], name: &str) -> Option<Level> {
    warnings
        .iter()
        .find(|warning| warning.airspace.name == name)
        .map(|warning| warning.level)
}

/// A straight, level stretch of flight at 10 m/s and 1200 m.
struct Leg {
    start: GeoPoint,
    course_deg: f64,
    seconds: u32,
}

/// Flies a leg and returns the level of the warning for `name` after every
/// second, with whether it was new.
fn fly(
    engine: &mut WarningEngine,
    index: &AirspaceIndex,
    name: &str,
    leg: &Leg,
) -> Vec<(Option<Level>, bool)> {
    (0..=leg.seconds)
        .map(|second| {
            let time = f64::from(second);
            let position = destination(leg.start, leg.course_deg, 10.0 * time).unwrap();
            let mut here = sample(time, position, 1200.0);
            here.ground_speed_ms = Some(10.0);
            here.course_deg = Some(leg.course_deg);
            let warnings = engine.update(index, &here).unwrap();
            let warning = warnings
                .iter()
                .find(|warning| warning.airspace.name == name);
            (warning.map(|w| w.level), warning.is_some_and(|w| w.is_new))
        })
        .collect()
}

#[test]
fn flying_into_a_control_zone_raises_near_then_entering_then_inside() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    // 3 km west of the zone's western edge, at its height, flying east.
    let edge = point(47.25, 11.2);
    let leg = Leg {
        start: destination(edge, 270.0, 3000.0).unwrap(),
        course_deg: 90.0,
        seconds: 320,
    };
    let flown = fly(&mut engine, &index, "CTR", &leg);

    let first = |level: Level| {
        flown
            .iter()
            .position(|&(seen, _)| seen == Some(level))
            .unwrap_or_else(|| panic!("{level:?} never raised"))
    };
    // Nothing beyond the outer margin of 2000 m.
    assert!(flown[..99].iter().all(|&(level, _)| level.is_none()));
    // Near at 2000 m: after 100 s.
    assert!(
        (99..=101).contains(&first(Level::Near)),
        "{}",
        first(Level::Near)
    );
    // Entering when the edge is a minute away: 600 m, after 240 s.
    assert!(
        (239..=241).contains(&first(Level::Entering)),
        "{}",
        first(Level::Entering)
    );
    // Inside at the edge: after 300 s.
    assert!(
        (299..=301).contains(&first(Level::Inside)),
        "{}",
        first(Level::Inside)
    );
    // "Very near" never shows on the way in: entering is the more urgent.
    assert!(
        flown
            .iter()
            .all(|&(level, _)| level != Some(Level::VeryNear))
    );

    // The warning only ever becomes more urgent, and is new exactly when it does.
    let mut previous = None;
    for &(level, is_new) in &flown {
        assert!(level >= previous, "{level:?} after {previous:?}");
        assert_eq!(is_new, level > previous);
        previous = level;
    }
}

#[test]
fn a_predicted_entry_says_when_and_which_way() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    let edge = point(47.25, 11.2);
    // 450 m from the edge at 15 m/s: half a minute out.
    let mut here = sample(0.0, destination(edge, 270.0, 450.0).unwrap(), 1200.0);
    here.ground_speed_ms = Some(15.0);
    here.course_deg = Some(90.0);
    let warnings = engine.update(&index, &here).unwrap();
    let ctr = warnings.iter().find(|w| w.airspace.name == "CTR").unwrap();
    assert_eq!(ctr.level, Level::Entering);
    let entry = ctr.entry.unwrap();
    assert!((entry.seconds - 30.0).abs() < 0.5, "{}", entry.seconds);
    assert_eq!(entry.via, EntryVia::Side);
    assert!(!ctr.horizontal.inside);
    assert!((ctr.horizontal.distance_m - 450.0).abs() < 1.0);
    assert!((ctr.horizontal.bearing_deg - 90.0).abs() < 0.5);

    // Flying away from it at the same distance: near, and no entry.
    let mut engine = WarningEngine::new(Config::default());
    here.course_deg = Some(270.0);
    let warnings = engine.update(&index, &here).unwrap();
    let ctr = warnings.iter().find(|w| w.airspace.name == "CTR").unwrap();
    assert_eq!(ctr.level, Level::Near);
    assert!(ctr.entry.is_none());

    // With no speed known nothing is predicted, and position alone decides.
    let mut engine = WarningEngine::new(Config::default());
    let close = sample(0.0, destination(edge, 270.0, 300.0).unwrap(), 1200.0);
    let warnings = engine.update(&index, &close).unwrap();
    assert_eq!(level_of(&warnings, "CTR"), Some(Level::VeryNear));
}

#[test]
fn passing_alongside_is_near_and_not_entering() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    // 300 m north of the zone's northern edge, flying east along it. Above
    // the pilot the terminal area's floor is 324 m away.
    let leg = Leg {
        start: destination(point(47.3, 11.25), 0.0, 300.0).unwrap(),
        course_deg: 90.0,
        seconds: 60,
    };
    let flown = fly(&mut engine, &index, "CTR", &leg);
    assert!(
        flown
            .iter()
            .all(|&(level, _)| level == Some(Level::VeryNear))
    );
    // New once, when it first appeared.
    assert_eq!(flown.iter().filter(|&&(_, is_new)| is_new).count(), 1);
}

#[test]
fn weaving_across_a_boundary_does_not_make_the_warning_flicker() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    let edge = point(47.25, 11.2);
    let mut news = 0;
    // A minute of crossing the western edge every second, 15 m either side.
    for second in 0..60 {
        let offset = if second % 2 == 0 { 15.0 } else { -15.0 };
        let position = destination(edge, 90.0, offset).unwrap();
        let warnings = engine
            .update(&index, &sample(f64::from(second), position, 1200.0))
            .unwrap();
        assert_eq!(
            level_of(&warnings, "CTR"),
            Some(Level::Inside),
            "second {second}"
        );
        news += warnings
            .iter()
            .filter(|w| w.is_new && w.airspace.name == "CTR")
            .count();
    }
    assert_eq!(news, 1);

    // Then the pilot leaves and stays 200 m outside. The warning holds for
    // ten seconds and then falls back, once.
    let outside = destination(edge, 270.0, 200.0).unwrap();
    let mut levels = Vec::new();
    for second in 60..90 {
        let warnings = engine
            .update(&index, &sample(f64::from(second), outside, 1200.0))
            .unwrap();
        levels.push(level_of(&warnings, "CTR").unwrap());
        assert!(
            warnings
                .iter()
                .all(|w| !w.is_new || w.airspace.name != "CTR")
        );
    }
    assert!(
        levels[..7].iter().all(|&level| level == Level::Inside),
        "{levels:?}"
    );
    assert!(
        levels[11..].iter().all(|&level| level == Level::VeryNear),
        "{levels:?}"
    );
    let changes = levels.windows(2).filter(|pair| pair[0] != pair[1]).count();
    assert_eq!(changes, 1);
}

#[test]
fn circling_in_a_drifting_thermal_is_predicted_by_the_drift() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    let edge = point(47.25, 11.2);
    // A thermal 1000 m west of the zone, drifting east at 3 m/s. The pilot
    // circles in it: 25 m radius, once around in 20 s, at 8 m/s.
    let mut levels = Vec::new();
    let mut entries = Vec::new();
    for second in 0..400 {
        let time = f64::from(second);
        let centre = destination(edge, 270.0, 1000.0 - 3.0 * time).unwrap();
        let around = time * 18.0;
        let position = destination(centre, around, 25.0).unwrap();
        let mut here = sample(time, position, 1200.0);
        here.ground_speed_ms = Some(8.0);
        here.course_deg = Some((around + 90.0).rem_euclid(360.0));
        let warnings = engine.update(&index, &here).unwrap();
        let ctr = warnings.iter().find(|w| w.airspace.name == "CTR").unwrap();
        levels.push(ctr.level);
        if ctr.level == Level::Entering {
            entries.push((time, ctr.entry));
        }
    }
    // The warning never falls back while the thermal carries the pilot in.
    assert!(
        levels.windows(2).all(|pair| pair[0] <= pair[1]),
        "{levels:?}"
    );
    assert_eq!(levels[0], Level::Near);
    assert_eq!(*levels.last().unwrap(), Level::Inside);
    // Entering is raised about a minute of drift before the edge (180 m,
    // less the 25 m the circle reaches ahead), not by the pilot's own 8 m/s
    // pointing at the zone for a moment two minutes earlier.
    let (first_entering, _) = entries[0];
    let edge_reached = (1000.0 - 25.0) / 3.0;
    assert!(
        (edge_reached - 75.0..edge_reached - 45.0).contains(&first_entering),
        "{first_entering}"
    );
}

#[test]
fn climbing_toward_a_floor_is_predicted_from_the_climb_rate() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    // In the terminal area's outline, west of the zone, climbing at 3 m/s
    // from 1100 m toward its floor at 1524 m.
    let position = point(47.25, 11.1);
    let mut seen = Vec::new();
    for second in 0..160 {
        let time = f64::from(second);
        let mut here = sample(time, position, 1100.0 + 3.0 * time);
        here.climb_ms = Some(3.0);
        let warnings = engine.update(&index, &here).unwrap();
        let tma = warnings.iter().find(|w| w.airspace.name == "TMA").copied();
        seen.push((
            here.altitude_m,
            tma.map(|w| w.level),
            tma.and_then(|w| w.entry),
        ));
    }
    let first = |level: Level| {
        seen.iter()
            .find(|&&(_, seen, _)| seen == Some(level))
            .unwrap()
    };

    // Near from the start: 424 m under the floor, within the 500 m margin.
    assert_eq!(seen[0].1, Some(Level::Near));
    // Entering a minute before the floor: 180 m under it.
    let (altitude, _, entry) = *first(Level::Entering);
    assert!((1524.0 - altitude - 180.0).abs() <= 3.0, "{altitude}");
    let entry = entry.unwrap();
    assert_eq!(entry.via, EntryVia::Floor);
    assert!((entry.seconds - 60.0).abs() <= 1.0);
    // Inside at the floor.
    let (altitude, _, _) = *first(Level::Inside);
    assert!((altitude - 1524.0).abs() <= 3.0, "{altitude}");
    // Never less urgent on the way up.
    assert!(seen.windows(2).all(|pair| pair[0].1 <= pair[1].1));

    // Sinking at the same height gives no entry: just under the floor is very near.
    let mut engine = WarningEngine::new(Config::default());
    let mut here = sample(0.0, position, 1450.0);
    here.climb_ms = Some(-2.0);
    let warnings = engine.update(&index, &here).unwrap();
    assert_eq!(level_of(&warnings, "TMA"), Some(Level::VeryNear));
}

#[test]
fn a_flight_level_without_a_barometer_warns_early_and_says_it_is_unsure() {
    let index = airspace();
    // South of everything else, 250 m under FL 100 by the satellites.
    let position = point(46.8, 10.5);

    // With a pressure altitude on a standard day: 248 m under it. Near.
    let mut engine = WarningEngine::new(Config::default());
    let warnings = engine
        .update(&index, &sample(0.0, position, 2800.0))
        .unwrap();
    let upper = warnings
        .iter()
        .find(|w| w.airspace.name == "UPPER")
        .unwrap();
    assert_eq!(upper.level, Level::Near);
    assert!(!upper.uncertain());
    assert!((upper.vertical.separation_m - 248.0).abs() < 1e-6);

    // The same position on a low-pressure day, the barometer reading
    // 260 m higher than the satellites: inside, and sure of it.
    let mut engine = WarningEngine::new(Config::default());
    let mut low_pressure = sample(0.0, position, 2800.0);
    low_pressure.pressure = Some(PressureAltitude::standard(3060.0));
    let warnings = engine.update(&index, &low_pressure).unwrap();
    let upper = warnings
        .iter()
        .find(|w| w.airspace.name == "UPPER")
        .unwrap();
    assert_eq!(upper.level, Level::Inside);
    assert!(!upper.uncertain());

    // Without a barometer nobody knows which of the two days it is. The
    // pilot is told "inside", marked uncertain.
    let mut engine = WarningEngine::new(Config::default());
    let mut no_barometer = sample(0.0, position, 2800.0);
    no_barometer.pressure = None;
    let warnings = engine.update(&index, &no_barometer).unwrap();
    let upper = warnings
        .iter()
        .find(|w| w.airspace.name == "UPPER")
        .unwrap();
    assert_eq!(upper.level, Level::Inside);
    assert!(upper.uncertain());
}

#[test]
fn open_airspace_and_glider_sectors_do_not_warn() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    // In class E, far from everything else, and well under FL 100.
    let warnings = engine
        .update(&index, &sample(0.0, point(46.8, 10.5), 2000.0))
        .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");

    // In the glider sector at 2000 m: the sector does not warn. The
    // terminal area it lies in does, because nobody knows whether the
    // sector is open.
    let warnings = engine
        .update(&index, &sample(1.0, point(47.15, 11.1), 2000.0))
        .unwrap();
    let names: Vec<&str> = warnings.iter().map(|w| w.airspace.name.as_str()).collect();
    assert_eq!(names, ["TMA"]);
    assert_eq!(warnings[0].level, Level::Inside);
}

#[test]
fn a_drop_zone_is_a_caution_only_when_flown_into() {
    let index = airspace();
    let south_edge = point(47.6, 11.25);

    // 300 m south of it, not moving toward it: nothing, where a control
    // zone would be "very near".
    let mut engine = WarningEngine::new(Config::default());
    let beside = destination(south_edge, 180.0, 300.0).unwrap();
    let warnings = engine.update(&index, &sample(0.0, beside, 1500.0)).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");

    // Flying toward it: entering.
    let mut heading_in = sample(1.0, beside, 1500.0);
    heading_in.ground_speed_ms = Some(10.0);
    heading_in.course_deg = Some(0.0);
    let warnings = engine.update(&index, &heading_in).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].level, Level::Entering);
    assert_eq!(warnings[0].policy, Policy::Caution);

    // In it.
    let warnings = engine
        .update(&index, &sample(2.0, point(47.62, 11.25), 1500.0))
        .unwrap();
    assert_eq!(level_of(&warnings, "DROP ZONE"), Some(Level::Inside));
}

#[test]
fn warnings_come_most_urgent_first() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    // In the control zone near its top: inside the zone, very near the
    // terminal area 24 m above, and FL 100 is far.
    let warnings = engine
        .update(&index, &sample(0.0, point(47.25, 11.35), 1500.0))
        .unwrap();
    let seen: Vec<(&str, Level)> = warnings
        .iter()
        .map(|w| (w.airspace.name.as_str(), w.level))
        .collect();
    assert_eq!(seen, [("CTR", Level::Inside), ("TMA", Level::VeryNear)]);
    assert_eq!(warnings[0].id(), &id("CTR"));
    assert!(warnings.iter().all(|w| w.is_new && w.since_s == 0.0));
}

#[test]
fn an_acknowledged_warning_is_quiet_until_it_gets_worse_or_the_time_is_up() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    let edge = point(47.25, 11.2);
    let near = destination(edge, 270.0, 1500.0).unwrap();

    let warnings = engine.update(&index, &sample(0.0, near, 1200.0)).unwrap();
    assert_eq!(level_of(&warnings, "CTR"), Some(Level::Near));
    engine.acknowledge(&id("CTR"), Silence::For { seconds: 600.0 });

    let warnings = engine.update(&index, &sample(1.0, near, 1200.0)).unwrap();
    let ctr = warnings.iter().find(|w| w.airspace.name == "CTR").unwrap();
    assert_eq!(ctr.silenced, Some(Silenced::Acknowledged));
    assert_eq!(ctr.silenced_until_s, Some(600.0));
    assert!(!ctr.is_new);
    // A silenced warning comes after those that speak.
    assert_eq!(warnings.last().unwrap().airspace.name, "CTR");

    // Closer, it is worse than what was acknowledged, and speaks.
    let closer = destination(edge, 270.0, 200.0).unwrap();
    let warnings = engine.update(&index, &sample(2.0, closer, 1200.0)).unwrap();
    let ctr = warnings.iter().find(|w| w.airspace.name == "CTR").unwrap();
    assert_eq!(ctr.level, Level::VeryNear);
    assert_eq!(ctr.silenced, None);
    assert!(ctr.is_new);

    // Back out to "near", the acknowledgement still holds.
    let mut time = 3.0;
    let ctr_at = |engine: &mut WarningEngine, time: f64| {
        let warnings = engine.update(&index, &sample(time, near, 1200.0)).unwrap();
        warnings
            .into_iter()
            .find(|w| w.airspace.name == "CTR")
            .unwrap()
    };
    while ctr_at(&mut engine, time).level != Level::Near {
        time += 1.0;
        assert!(time < 60.0);
    }
    assert_eq!(
        ctr_at(&mut engine, time + 1.0).silenced,
        Some(Silenced::Acknowledged)
    );

    // After ten minutes it speaks again, once.
    let after = ctr_at(&mut engine, 601.0);
    assert_eq!(after.silenced, None);
    assert!(after.is_new);
    assert!(!ctr_at(&mut engine, 602.0).is_new);

    // Acknowledging what is not there does nothing.
    engine.acknowledge(&id("DROP ZONE"), Silence::For { seconds: 600.0 });
    assert_eq!(ctr_at(&mut engine, 603.0).silenced, None);
}

#[test]
fn an_ignored_airspace_stays_quiet_whatever_happens_until_the_day_ends() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    let inside = point(47.25, 11.35);
    // 2026-07-01 13:00:00 UTC, which is 15:00 in Innsbruck in summer.
    let afternoon = 1_782_910_800.0;
    let warnings = engine
        .update(&index, &sample(afternoon, inside, 1200.0))
        .unwrap();
    assert_eq!(level_of(&warnings, "CTR"), Some(Level::Inside));

    // "The tower has cleared us into the zone today."
    engine.ignore(&id("CTR"), Silence::RestOfDay { utc_offset_s: 7200 });
    let warnings = engine
        .update(&index, &sample(afternoon + 1.0, inside, 1200.0))
        .unwrap();
    let ctr = warnings.iter().find(|w| w.airspace.name == "CTR").unwrap();
    assert_eq!(ctr.level, Level::Inside);
    assert_eq!(ctr.silenced, Some(Silenced::Ignored));
    // Local midnight is 22:00 UTC: nine hours after 13:00.
    assert_eq!(ctr.silenced_until_s, Some(afternoon + 9.0 * 3600.0));
    // The terminal area above was not ignored and still speaks.
    assert_eq!(warnings[0].airspace.name, "TMA");
    assert_eq!(warnings[0].silenced, None);

    // Still quiet in the evening, speaking again after midnight.
    let evening = afternoon + 8.0 * 3600.0;
    let warnings = engine
        .update(&index, &sample(evening, inside, 1200.0))
        .unwrap();
    assert_eq!(warnings.last().unwrap().silenced, Some(Silenced::Ignored));
    let next_day = afternoon + 9.0 * 3600.0 + 1.0;
    let warnings = engine
        .update(&index, &sample(next_day, inside, 1200.0))
        .unwrap();
    let ctr = warnings.iter().find(|w| w.airspace.name == "CTR").unwrap();
    assert_eq!(ctr.silenced, None);
    assert!(ctr.is_new);

    // Ignoring until a given time, and taking it back.
    engine.ignore(
        &id("CTR"),
        Silence::Until {
            time_s: next_day + 100.0,
        },
    );
    let warnings = engine
        .update(&index, &sample(next_day + 1.0, inside, 1200.0))
        .unwrap();
    assert_eq!(warnings.last().unwrap().silenced, Some(Silenced::Ignored));
    engine.unsilence(&id("CTR"));
    let warnings = engine
        .update(&index, &sample(next_day + 2.0, inside, 1200.0))
        .unwrap();
    assert!(warnings.iter().all(|w| w.silenced.is_none()));
}

#[test]
fn samples_that_are_not_numbers_are_refused_and_a_rewind_starts_afresh() {
    let index = airspace();
    let mut engine = WarningEngine::new(Config::default());
    let inside = point(47.25, 11.35);

    let mut broken = sample(0.0, inside, f64::NAN);
    assert_eq!(engine.update(&index, &broken), Err(SampleError::Altitude));
    broken = sample(f64::INFINITY, inside, 1200.0);
    assert_eq!(engine.update(&index, &broken), Err(SampleError::Time));

    // Speeds and climbs that are not numbers are treated as unknown.
    let mut odd = sample(10.0, inside, 1200.0);
    odd.ground_speed_ms = Some(f64::NAN);
    odd.course_deg = Some(f64::INFINITY);
    odd.climb_ms = Some(f64::NAN);
    let warnings = engine.update(&index, &odd).unwrap();
    assert_eq!(level_of(&warnings, "CTR"), Some(Level::Inside));
    assert!(warnings[0].is_new);
    assert!(
        !engine
            .update(&index, &sample(11.0, inside, 1200.0))
            .unwrap()[0]
            .is_new
    );

    // A replay rewound to an earlier time: the warning is new again.
    let warnings = engine.update(&index, &sample(5.0, inside, 1200.0)).unwrap();
    assert!(warnings[0].is_new);
    assert_eq!(warnings[0].since_s, 5.0);
}

#[test]
fn the_same_flight_gives_the_same_warnings_every_time() {
    let index = airspace();
    let fly = || {
        let mut engine = WarningEngine::new(Config::default());
        let start = destination(point(47.25, 11.2), 250.0, 2500.0).unwrap();
        let mut seen = Vec::new();
        for second in 0..400 {
            let time = f64::from(second);
            let position = destination(start, 70.0, 9.0 * time).unwrap();
            let mut here = sample(time, position, 1000.0 + time);
            here.climb_ms = Some(1.0);
            for warning in engine.update(&index, &here).unwrap() {
                seen.push(format!(
                    "{time} {} {:?} {} {:?}",
                    warning.id(),
                    warning.level,
                    warning.is_new,
                    warning.entry
                ));
            }
        }
        seen
    };
    let first = fly();
    assert!(first.len() > 400);
    assert_eq!(first, fly());
}
