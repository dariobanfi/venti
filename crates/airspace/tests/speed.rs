//! Measures how long the questions take, on a country's worth of airspace.
//!
//! The cockpit asks once a second on a phone, so an answer has to take far
//! less than a second. This test builds a made-up country of 3,000
//! airspaces, somewhat denser than Germany and Austria together, asks every
//! question many times and fails if any is slower than a generous limit.
//! The limits are for an unoptimised build on a busy machine; a release
//! build was four to seven times faster when this was written.
//!
//! To see the numbers, and to measure real files as well:
//!
//! ```text
//! cargo test -p venti-airspace --release --test speed -- --nocapture
//! VENTI_AIRSPACE_FILES=de.txt,at.txt cargo test -p venti-airspace --release --test speed -- --nocapture
//! ```
//!
//! The files are OpenAir files; they are not part of the repository. The
//! numbers measured on openAIP's exports for Germany and Austria are in
//! `docs/airspace.md`.

use std::time::{Duration, Instant};

use venti_airspace::openair::{self, Options};
use venti_airspace::{
    AirspaceIndex, Altitudes, Config, PressureAltitude, Sample, WarningEngine, circle_ring,
    destination,
};
use venti_domain::{
    Activity, Airspace, AirspaceClass, AirspaceType, GeoPoint, HeightUnit, Polygon, SourceId,
    Validity, VerticalLimit, VerticalReference,
};

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).unwrap()
}

/// A country that does not exist: 3,000 airspaces between 47 and 55 degrees
/// north and 6 and 15 degrees east, with about 220,000 corners.
fn made_up_country() -> Vec<Airspace> {
    let mut random = fastrand::Rng::with_seed(2026);
    let feet =
        |reference, value: f64| VerticalLimit::new(reference, value, HeightUnit::Feet).unwrap();
    let limits = [
        (
            VerticalLimit::ground(),
            feet(VerticalReference::MeanSeaLevel, 4500.0),
        ),
        (
            VerticalLimit::ground(),
            VerticalLimit::flight_level(100).unwrap(),
        ),
        (
            feet(VerticalReference::AboveGround, 1000.0),
            VerticalLimit::flight_level(65).unwrap(),
        ),
        (
            feet(VerticalReference::MeanSeaLevel, 3500.0),
            VerticalLimit::flight_level(100).unwrap(),
        ),
        (
            VerticalLimit::flight_level(100).unwrap(),
            VerticalLimit::unlimited(),
        ),
    ];
    let kinds = [
        (AirspaceClass::D, AirspaceType::Ctr),
        (AirspaceClass::C, AirspaceType::Tma),
        (AirspaceClass::Unclassified, AirspaceType::Restricted),
        (AirspaceClass::E, AirspaceType::Tmz),
        (AirspaceClass::Unclassified, AirspaceType::AerialSporting),
    ];

    let mut airspaces = Vec::new();
    for id in 0..3000 {
        let centre = point(47.0 + 8.0 * random.f64(), 6.0 + 9.0 * random.f64());
        let star = |random: &mut fastrand::Rng, corners: usize, reach_m: f64| {
            let mut ring: Vec<GeoPoint> = (0..corners)
                .map(|corner| {
                    let bearing = 360.0 * corner as f64 / corners as f64;
                    destination(centre, bearing, reach_m * (0.5 + 0.5 * random.f64())).unwrap()
                })
                .collect();
            ring.push(ring[0]);
            ring
        };
        let ring = match id {
            // A few regions that cover a large part of the country.
            0..20 => {
                let corners = random.usize(1000..3000);
                star(&mut random, corners, 300_000.0)
            }
            // Terminal areas and long borders.
            20..300 => {
                let corners = random.usize(50..400);
                let reach = 20_000.0 + 40_000.0 * random.f64();
                star(&mut random, corners, reach)
            }
            // Circles of two to eight nautical miles.
            300..1200 => circle_ring(centre, 1852.0 * (2.0 + 6.0 * random.f64())).unwrap(),
            // Everything else: zones and sectors of a few kilometres.
            _ => {
                let corners = random.usize(4..60);
                let reach = 2_000.0 + 23_000.0 * random.f64();
                star(&mut random, corners, reach)
            }
        };
        let (lower, upper) = limits[random.usize(0..limits.len())];
        let (class, airspace_type) = kinds[random.usize(0..kinds.len())].clone();
        airspaces.push(Airspace {
            id: format!("made-up:{id}").parse().unwrap(),
            name: format!("AIRSPACE {id}"),
            class,
            airspace_type,
            country: Some("DE".to_owned()),
            lower,
            upper,
            lower_at_least: None,
            upper_at_most: None,
            geometry: Polygon::new(ring, Vec::new()).unwrap(),
            activity: Activity::default(),
            frequencies: Vec::new(),
            remarks: None,
            validity: Validity::default(),
        });
    }
    airspaces
}

/// How long the measured flight lasts, at one sample a second.
const FLIGHT_S: usize = 900;

/// What one question cost: the mean and the slowest of its calls.
struct Cost {
    mean: Duration,
    slowest: Duration,
}

fn time<T>(calls: usize, mut call: impl FnMut(usize) -> T) -> Cost {
    let mut total = Duration::ZERO;
    let mut slowest = Duration::ZERO;
    for index in 0..calls {
        let started = Instant::now();
        std::hint::black_box(call(index));
        let taken = started.elapsed();
        total += taken;
        slowest = slowest.max(taken);
    }
    Cost {
        mean: total / calls as u32,
        slowest,
    }
}

fn microseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e6
}

/// Asks every question many times around `centre` and prints what each
/// cost. Returns the mean cost of the question the cockpit asks every
/// second: one update of the warning engine.
fn measure(label: &str, airspaces: Vec<Airspace>, centre: GeoPoint) -> Duration {
    let started = Instant::now();
    let index = AirspaceIndex::new(airspaces);
    let build = started.elapsed();
    let stats = index.stats();

    let mut random = fastrand::Rng::with_seed(1);
    // Positions within about 100 km of the centre, where the flying is.
    let probes: Vec<GeoPoint> = (0..400)
        .map(|_| destination(centre, 360.0 * random.f64(), 100_000.0 * random.f64()).unwrap())
        .collect();
    let altitudes = Altitudes {
        altitude_m: 1800.0,
        pressure: Some(PressureAltitude::standard(1800.0)),
        ground_elevation_m: Some(700.0),
    };

    let mut overhead = 0;
    let at = time(probes.len(), |call| {
        let found = index.at(probes[call], &altitudes);
        overhead += found.len();
        found.len()
    });
    let near_2 = time(probes.len(), |call| {
        index.nearby(probes[call], &altitudes, 2000.0, 500.0).len()
    });
    let near_20 = time(probes.len(), |call| {
        index
            .nearby(probes[call], &altitudes, 20_000.0, 500.0)
            .len()
    });
    let section = time(60, |call| {
        let path = [probes[call], probes[call + 1], probes[call + 2]];
        index.cross_section(&path).len()
    });

    // A quarter of an hour of flight at one sample a second: south from the
    // centre at 11 m/s, climbing and sinking between 1000 m and 2600 m.
    let mut engine = WarningEngine::new(Config::default());
    let mut raised = 0;
    let update = time(FLIGHT_S, |second| {
        let seconds = second as f64;
        let altitude = 1800.0 + 800.0 * (seconds / 300.0).sin();
        let sample = Sample {
            time_s: seconds,
            position: destination(centre, 170.0, 11.0 * seconds).unwrap(),
            altitude_m: altitude,
            pressure: Some(PressureAltitude::standard(altitude)),
            ground_elevation_m: Some(700.0),
            ground_speed_ms: Some(11.0),
            course_deg: Some(170.0),
            climb_ms: Some(2.7 * (seconds / 300.0).cos()),
        };
        let warnings = engine.update(&index, &sample).unwrap();
        raised += warnings.len();
        warnings.len()
    });

    println!("{label}");
    println!(
        "  {} airspaces, {} corners, {} cells with {} entries, about {} KB in memory",
        stats.airspaces,
        stats.points,
        stats.cells,
        stats.cell_entries,
        stats.bytes / 1024
    );
    println!(
        "  building the index             {:>9.0} us",
        microseconds(build)
    );
    let line = |name: &str, cost: &Cost| {
        println!(
            "  {name:<30} {:>9.1} us mean {:>9.1} us slowest",
            microseconds(cost.mean),
            microseconds(cost.slowest)
        );
    };
    line("at a position", &at);
    line("nearby within 2 km", &near_2);
    line("nearby within 20 km", &near_20);
    line("side view along two legs", &section);
    line("one update of the warnings", &update);
    println!(
        "  on average {:.1} airspaces overhead and {:.1} warnings per second of flight",
        overhead as f64 / probes.len() as f64,
        raised as f64 / FLIGHT_S as f64
    );

    // Limits for an unoptimised build on a busy, shared machine: ten to
    // twenty times what such a build takes on an idle one, so that only a
    // real slowdown fails the test and a noisy neighbour does not.
    assert!(at.mean < Duration::from_millis(10), "{:?}", at.mean);
    assert!(near_2.mean < Duration::from_millis(25), "{:?}", near_2.mean);
    assert!(
        near_20.mean < Duration::from_millis(100),
        "{:?}",
        near_20.mean
    );
    assert!(
        section.mean < Duration::from_millis(250),
        "{:?}",
        section.mean
    );
    update.mean
}

#[test]
fn a_country_of_airspace_is_answered_in_a_fraction_of_a_second() {
    let update = measure("A made-up country", made_up_country(), point(50.0, 10.0));
    // Once a second on a phone: even unoptimised this must be far below.
    assert!(update < Duration::from_millis(60), "{update:?}");
}

#[test]
fn real_files_are_measured_when_given() {
    let Ok(files) = std::env::var("VENTI_AIRSPACE_FILES") else {
        return;
    };
    let mut airspaces = Vec::new();
    for (number, path) in files.split(',').enumerate() {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let source = SourceId::new(format!("file-{number}")).unwrap();
        let started = Instant::now();
        let file = openair::parse_bytes(&bytes, &Options::new(source));
        println!(
            "{path}: {} KB read in {:.1} ms: {} airspaces, {} problems",
            bytes.len() / 1024,
            started.elapsed().as_secs_f64() * 1e3,
            file.airspaces.len(),
            file.problems.len() + file.problems_omitted
        );
        for problem in file.problems.iter().take(5) {
            println!(
                "  line {}: {:?}: {}",
                problem.line, problem.kind, problem.message
            );
        }
        airspaces.extend(file.airspaces);
    }
    // From near Munich airport south across the city, through its control zone and terminal area.
    measure("The files given", airspaces, point(48.3, 11.75));
}
