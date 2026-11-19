//! Checks that hold for any input, tried on thousands of generated cases.
//!
//! The geometry is checked against a second, independent way of working out
//! the same answer. The file reader is fed files that are cut off, shuffled,
//! damaged and hostile, and must neither panic nor return a record that is
//! not valid. Every case comes from a fixed seed, so a failure repeats.

use venti_airspace::openair::{self, Options, ProblemKind};
use venti_airspace::{
    ARC_MAX_ERROR_M, AirspaceIndex, Altitudes, METRES_PER_NAUTICAL_MILE, VerticalRelation,
    destination, horizontal,
};
use venti_domain::{
    Activity, Airspace, AirspaceClass, AirspaceType, GeoPoint, Polygon, SourceId, Validity,
    VerticalLimit,
};

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).unwrap()
}

fn airspace(id: usize, ring: Vec<GeoPoint>) -> Airspace {
    Airspace {
        id: format!("test:{id}").parse().unwrap(),
        name: format!("AIRSPACE {id}"),
        class: AirspaceClass::D,
        airspace_type: AirspaceType::Ctr,
        country: None,
        lower: VerticalLimit::ground(),
        upper: VerticalLimit::unlimited(),
        lower_at_least: None,
        upper_at_most: None,
        geometry: Polygon::new(ring, Vec::new()).unwrap(),
        activity: Activity::default(),
        frequencies: Vec::new(),
        remarks: None,
        validity: Validity::default(),
    }
}

/// A ring that winds once around `centre` at distances that vary from
/// corner to corner: never crossing itself, often far from convex. It has
/// some number of corners out of `corners`.
fn star(
    random: &mut fastrand::Rng,
    centre: GeoPoint,
    corners: std::ops::Range<usize>,
    reach_m: f64,
) -> Vec<GeoPoint> {
    let corners = random.usize(corners);
    let mut ring: Vec<GeoPoint> = (0..corners)
        .map(|corner| {
            let bearing = 360.0 * corner as f64 / corners as f64;
            let distance = reach_m * (0.2 + 0.8 * random.f64());
            destination(centre, bearing, distance).unwrap()
        })
        .collect();
    ring.push(ring[0]);
    ring
}

/// How often a ring winds around a point: zero outside. Counts the edges
/// that pass the point on its east side going up and going down, by the
/// sign of a cross product. It shares no formula with the crate's own test,
/// which counts crossings of a ray by where they fall.
fn winding_number(ring: &[GeoPoint], at: GeoPoint) -> i32 {
    let side = |a: GeoPoint, b: GeoPoint| {
        (b.lon() - a.lon()) * (at.lat() - a.lat()) - (at.lon() - a.lon()) * (b.lat() - a.lat())
    };
    let mut winding = 0;
    for pair in ring.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a.lat() <= at.lat() {
            if b.lat() > at.lat() && side(a, b) > 0.0 {
                winding += 1;
            }
        } else if b.lat() <= at.lat() && side(a, b) < 0.0 {
            winding -= 1;
        }
    }
    winding
}

/// The distance to a ring by brute force: the nearest of many points
/// spread along every edge, each measured on the sphere.
fn distance_by_sampling(ring: &[GeoPoint], from: GeoPoint) -> f64 {
    let mut nearest = f64::INFINITY;
    for pair in ring.windows(2) {
        for step in 0..=400 {
            let along = f64::from(step) / 400.0;
            let on_edge = point(
                pair[0].lat() + along * (pair[1].lat() - pair[0].lat()),
                pair[0].lon() + along * (pair[1].lon() - pair[0].lon()),
            );
            nearest = nearest.min(from.distance_m(on_edge));
        }
    }
    nearest
}

#[test]
fn inside_agrees_with_an_independent_winding_number() {
    let mut random = fastrand::Rng::with_seed(20_261_119);
    let mut inside_count = 0;
    for case in 0..300 {
        let centre = point(46.0 + 3.0 * random.f64(), 9.0 + 8.0 * random.f64());
        let ring = star(&mut random, centre, 3..40, 20_000.0);
        let index = AirspaceIndex::new(vec![airspace(case, ring.clone())]);
        for _ in 0..200 {
            let probe = destination(centre, 360.0 * random.f64(), 25_000.0 * random.f64()).unwrap();
            let reported = !index.at(probe, &Altitudes::gps(1000.0)).is_empty();
            let expected = winding_number(&ring, probe) != 0;
            assert_eq!(reported, expected, "case {case} at {probe:?}");
            inside_count += usize::from(reported);

            // The single-airspace answer says the same.
            let relation = horizontal(index.iter().next().unwrap(), probe).unwrap();
            assert_eq!(relation.inside, expected);
        }
    }
    // The probes fell on both sides often enough to mean something.
    assert!((10_000..50_000).contains(&inside_count), "{inside_count}");
}

#[test]
fn the_distance_to_a_boundary_matches_brute_force() {
    let mut random = fastrand::Rng::with_seed(7);
    for case in 0..60 {
        let centre = point(46.0 + 3.0 * random.f64(), 9.0 + 8.0 * random.f64());
        let ring = star(&mut random, centre, 3..25, 15_000.0);
        let shape = airspace(case, ring.clone());
        for _ in 0..20 {
            let probe = destination(centre, 360.0 * random.f64(), 40_000.0 * random.f64()).unwrap();
            let relation = horizontal(&shape, probe).unwrap();
            let expected = distance_by_sampling(&ring, probe);
            // The sampling is coarse by up to a four-hundredth of an edge;
            // the flat map by half a percent at this range.
            let tolerance = 40.0 + 0.005 * expected;
            assert!(
                (relation.distance_m - expected).abs() <= tolerance,
                "case {case}: {} against {expected}",
                relation.distance_m
            );
            // The nearest point is on the boundary, at the distance and in
            // the direction reported.
            assert!(distance_by_sampling(&ring, relation.nearest) < 40.0);
            assert!((probe.distance_m(relation.nearest) - relation.distance_m).abs() < 1e-6);
            if relation.distance_m > 100.0 {
                let there = destination(probe, relation.bearing_deg, relation.distance_m).unwrap();
                assert!(there.distance_m(relation.nearest) < 0.01 * relation.distance_m + 1.0);
            }
        }
    }
}

#[test]
fn the_distance_is_zero_on_the_boundary() {
    let mut random = fastrand::Rng::with_seed(11);
    for case in 0..100 {
        let centre = point(46.0 + 3.0 * random.f64(), 9.0 + 8.0 * random.f64());
        let ring = star(&mut random, centre, 3..25, 15_000.0);
        let shape = airspace(case, ring.clone());
        for pair in ring.windows(2) {
            let along = random.f64();
            let on_edge = point(
                pair[0].lat() + along * (pair[1].lat() - pair[0].lat()),
                pair[0].lon() + along * (pair[1].lon() - pair[0].lon()),
            );
            for on_boundary in [pair[0], on_edge] {
                let relation = horizontal(&shape, on_boundary).unwrap();
                assert!(relation.distance_m < 0.001, "{}", relation.distance_m);
            }
            // A corner is on the boundary, and the boundary belongs to the airspace.
            assert!(horizontal(&shape, pair[0]).unwrap().inside);
        }
    }
}

#[test]
fn distances_are_symmetric_where_the_shape_is() {
    // A square around a point: probes mirrored through its centre line are
    // equally far from it, east against west and north against south.
    let (lat, lon) = (47.5, 11.5);
    let half = 0.1;
    let ring = vec![
        point(lat - half, lon - half),
        point(lat - half, lon + half),
        point(lat + half, lon + half),
        point(lat + half, lon - half),
        point(lat - half, lon - half),
    ];
    let square = airspace(0, ring);
    let mut random = fastrand::Rng::with_seed(3);
    for _ in 0..500 {
        let (north, east) = (0.4 * random.f64() - 0.2, 0.4 * random.f64() - 0.2);
        let here = horizontal(&square, point(lat + north, lon + east)).unwrap();
        let mirrored_east = horizontal(&square, point(lat + north, lon - east)).unwrap();
        assert!((here.distance_m - mirrored_east.distance_m).abs() < 1e-6);
        assert_eq!(here.inside, mirrored_east.inside);
        if here.distance_m > 1.0 {
            let mirrored = (360.0 - here.bearing_deg).rem_euclid(360.0);
            let difference = (mirrored - mirrored_east.bearing_deg).abs();
            assert!(difference.min(360.0 - difference) < 1e-6);
        }
        // North against south the globe itself is not symmetric: a degree
        // of longitude is shorter farther north. Straight above and below
        // the square, where the distance runs along a meridian, it is.
        if north.abs() > half && east.abs() < half {
            let mirrored_north = horizontal(&square, point(lat - north, lon + east)).unwrap();
            assert!((here.distance_m - mirrored_north.distance_m).abs() < 1e-6);
            let expected = (north.abs() - half) * 111_194.926_6;
            assert!(
                (here.distance_m - expected).abs() < 0.01,
                "{}",
                here.distance_m
            );
        }
    }

    // A circle of five nautical miles: from twice the radius out, the
    // boundary is one radius away in every direction, and straight back.
    let centre = point(47.5, 11.5);
    let radius = 5.0 * METRES_PER_NAUTICAL_MILE;
    let ring = venti_airspace::circle_ring(centre, radius).unwrap();
    let circle = airspace(1, ring);
    for degree in 0..360 {
        let bearing = f64::from(degree);
        let outside = destination(centre, bearing, 2.0 * radius).unwrap();
        let relation = horizontal(&circle, outside).unwrap();
        assert!(!relation.inside);
        assert!((relation.distance_m - radius).abs() <= ARC_MAX_ERROR_M + 0.003 * radius);
        let back = (bearing + 180.0).rem_euclid(360.0);
        let difference = (relation.bearing_deg - back).abs();
        assert!(
            difference.min(360.0 - difference) < 4.0,
            "{degree}: {}",
            relation.bearing_deg
        );
        // From the centre it is one radius to the edge, whichever way.
        let from_centre = horizontal(&circle, centre).unwrap();
        assert!(from_centre.inside);
        assert!((from_centre.distance_m - radius).abs() <= ARC_MAX_ERROR_M);
    }
}

#[test]
fn every_question_gives_the_same_picture() {
    let mut random = fastrand::Rng::with_seed(99);
    let airspaces: Vec<Airspace> = (0..200)
        .map(|id| {
            let centre = point(46.5 + 2.0 * random.f64(), 10.0 + 4.0 * random.f64());
            let ring = star(&mut random, centre, 3..30, 12_000.0);
            airspace(id, ring)
        })
        .collect();
    let index = AirspaceIndex::new(airspaces.clone());
    let altitudes = Altitudes::gps(1500.0);

    for _ in 0..300 {
        let probe = point(46.4 + 2.2 * random.f64(), 9.9 + 4.2 * random.f64());
        // What the index finds at a point is what looking at every airspace finds.
        let mut found: Vec<String> = index
            .at(probe, &altitudes)
            .iter()
            .map(|entry| {
                assert_eq!(entry.vertical.relation, VerticalRelation::Inside);
                entry.airspace.id.to_string()
            })
            .collect();
        found.sort();
        let mut expected: Vec<String> = airspaces
            .iter()
            .filter(|airspace| winding_number(airspace.geometry.exterior(), probe) != 0)
            .map(|airspace| airspace.id.to_string())
            .collect();
        expected.sort();
        assert_eq!(found, expected);

        // What is near includes what is at the point, and nothing farther
        // than asked; it is the same set looking at every airspace gives.
        let radius = 8000.0 * random.f64();
        let near = index.nearby(probe, &altitudes, radius, 0.0);
        let mut near_ids: Vec<String> = near.iter().map(|n| n.airspace.id.to_string()).collect();
        near_ids.sort();
        let mut expected: Vec<String> = airspaces
            .iter()
            .filter(|airspace| {
                let relation = horizontal(airspace, probe).unwrap();
                relation.inside || relation.distance_m <= radius
            })
            .map(|airspace| airspace.id.to_string())
            .collect();
        expected.sort();
        assert_eq!(near_ids, expected);
        assert!(near.windows(2).all(|pair| {
            pair[0].horizontal.separation_m() <= pair[1].horizontal.separation_m()
        }));
    }

    // A side view agrees with asking point by point along the path.
    for _ in 0..40 {
        let from = point(46.5 + 2.0 * random.f64(), 10.0 + 4.0 * random.f64());
        let to = point(46.5 + 2.0 * random.f64(), 10.0 + 4.0 * random.f64());
        let length = from.distance_m(to);
        let spans = index.cross_section(&[from, to]);
        for step in 1..200 {
            let along = f64::from(step) / 200.0;
            let here = point(
                from.lat() + along * (to.lat() - from.lat()),
                from.lon() + along * (to.lon() - from.lon()),
            );
            let metres = along * length;
            for airspace in &airspaces {
                let over = winding_number(airspace.geometry.exterior(), here) != 0;
                let in_span = spans.iter().any(|span| {
                    span.airspace.id == airspace.id && span.from_m <= metres && metres <= span.to_m
                });
                // Within a metre of where a span begins or ends, either answer is right.
                let at_an_end = spans.iter().any(|span| {
                    span.airspace.id == airspace.id
                        && ((span.from_m - metres).abs() < 1.0 || (span.to_m - metres).abs() < 1.0)
                });
                assert!(over == in_span || at_an_end, "{} at {metres}", airspace.id);
            }
        }
    }
}

#[test]
fn the_ends_of_the_earth_do_not_panic() {
    let mut random = fastrand::Rng::with_seed(5);
    // Airspaces at the poles, on the antimeridian and on the equator.
    let mut airspaces = Vec::new();
    for (id, (lat, lon)) in [
        (89.0, 0.0),
        (-89.0, 100.0),
        (0.0, 179.0),
        (0.0, -179.5),
        (47.0, 11.0),
    ]
    .into_iter()
    .enumerate()
    {
        let ring = vec![
            point(lat, lon),
            point(lat, lon + 0.5),
            point(lat + 0.5, lon + 0.5),
            point(lat + 0.5, lon),
            point(lat, lon),
        ];
        airspaces.push(airspace(id, ring));
    }
    let index = AirspaceIndex::new(airspaces);
    let extremes = [-90.0, -89.999, 0.0, 89.999, 90.0];
    let meridians = [-180.0, -179.999, 0.0, 179.999, 180.0];
    for lat in extremes {
        for lon in meridians {
            let here = point(lat, lon);
            let altitudes = Altitudes::gps(random.f64() * 5000.0);
            let _ = index.at(here, &altitudes);
            let _ = index.nearby(here, &altitudes, 100_000.0, 1000.0);
            let _ = index.cross_section(&[here, point(-lat, -lon), point(0.0, 0.0)]);
            for airspace in index.iter() {
                let _ = horizontal(airspace, here);
            }
        }
    }
    assert_eq!(index.at(point(89.2, 0.2), &Altitudes::gps(0.0)).len(), 1);
}

/// A file with every kind of line the reader knows.
const SAMPLE: &str = "\
* A sample file
*VERSION: 2.1
*WRITTEN_BY: test

AC D
AY CTR
AN CTR STADIUM
AF 120.100
AG STADIUM TOWER
AL GND
AH 4500ft MSL
V X=47:30:00 N 011:00:00 E
DA 3, 180, 0
V X=47:30:00 N 011:20:00 E
DB 47:33:00 N 011:20:00 E, 47:27:00 N 011:20:00 E

AC Q
AN PARA Ailertchen EDGA
AH FL100
AL GND
V X=50:35:36N 007:56:42E
DC 5.00

AC E
AY RMZ
AN RMZ ETMN-GLIDER HX
AA NONE
AX 7000
AH FL145
AL 1000ft AGL
DP 53:47.100N 008:21.700E
DP 53:50.960N 008:55.825E
DP 53:45.800N 008:57.211E
DP 53:41.755N 008:23.890E
DP 53:47.100N 008:21.700E

AC R
AN BITTEN * with a comment
AL 1500m AMSL
AH UNLIM
DP 47:40:00 N 011:00:00 E
DP 47:40:00 N 011:20:00 E
V X=47:30:00 N 011:20:00 E
V D=-
DA 2, 0, 180
DP 47:20:00 N 011:20:00 E
DP 47:20:00 N 011:00:00 E
";

fn options() -> Options {
    Options::new(SourceId::new("fuzz").unwrap())
}

/// Reads a text and checks that whatever comes back is sound.
fn read_soundly(text: &str) -> openair::OpenAir {
    let file = openair::parse(text, &options());
    for airspace in &file.airspaces {
        assert_eq!(airspace.validate(), Ok(()), "{}", airspace.name);
    }
    let lines = text.lines().count();
    for problem in &file.problems {
        assert!(
            (1..=lines.max(1)).contains(&problem.line),
            "line {}",
            problem.line
        );
        assert!(!problem.message.is_empty());
    }
    file
}

#[test]
fn the_sample_file_reads_without_a_problem() {
    let file = read_soundly(SAMPLE);
    assert_eq!(file.problems, Vec::new());
    assert_eq!(file.airspaces.len(), 4);
}

#[test]
fn a_file_cut_off_anywhere_does_not_panic() {
    let whole = read_soundly(SAMPLE).airspaces.len();
    for end in 0..=SAMPLE.len() {
        if !SAMPLE.is_char_boundary(end) {
            continue;
        }
        let file = read_soundly(&SAMPLE[..end]);
        assert!(file.airspaces.len() <= whole);
        // The same cut seen as bytes, as a shell reading a file would pass it.
        let bytes = openair::parse_bytes(&SAMPLE.as_bytes()[..end], &options());
        assert_eq!(bytes.airspaces.len(), file.airspaces.len());
    }
}

#[test]
fn a_file_with_its_lines_shuffled_does_not_panic() {
    let mut random = fastrand::Rng::with_seed(42);
    let mut skipped = 0;
    for _ in 0..500 {
        let mut lines: Vec<&str> = SAMPLE.lines().collect();
        random.shuffle(&mut lines);
        let file = read_soundly(&lines.join("\n"));
        skipped += file
            .problems
            .iter()
            .filter(|problem| problem.kind == ProblemKind::AirspaceSkipped)
            .count();
    }
    // Shuffling breaks shapes, and broken shapes are reported, not kept quiet.
    assert!(skipped > 500, "{skipped}");
}

#[test]
fn a_damaged_file_does_not_panic() {
    let mut random = fastrand::Rng::with_seed(1234);
    let replacements: Vec<char> = "0123456789:., NSEWnsew=+-*ADCVPBYFLGUXZ\n\t\u{e4}\u{0}/"
        .chars()
        .collect();
    for _ in 0..3000 {
        let mut characters: Vec<char> = SAMPLE.chars().collect();
        for _ in 0..random.usize(1..6) {
            let at = random.usize(0..characters.len());
            match random.u8(0..3) {
                0 => characters[at] = replacements[random.usize(0..replacements.len())],
                1 => {
                    characters.remove(at);
                }
                _ => characters.insert(at, replacements[random.usize(0..replacements.len())]),
            }
        }
        let damaged: String = characters.into_iter().collect();
        read_soundly(&damaged);
    }
}

#[test]
fn noise_does_not_panic() {
    let mut random = fastrand::Rng::with_seed(77);
    // Arbitrary bytes.
    for _ in 0..500 {
        let bytes: Vec<u8> = (0..random.usize(0..400)).map(|_| random.u8(..)).collect();
        let file = openair::parse_bytes(&bytes, &options());
        for airspace in &file.airspaces {
            assert_eq!(airspace.validate(), Ok(()));
        }
    }
    // Lines made of the format's own words with numbers of every size.
    let commands = [
        "AC", "AN", "AY", "AL", "AH", "AF", "AG", "AA", "AX", "AI", "V", "V X=", "V D=", "V W=",
        "DP", "DA", "DB", "DC", "DY", "DZ", "D", "*", "",
    ];
    let words = [
        "R",
        "D",
        "CTR",
        "GND",
        "UNL",
        "FL",
        "ft",
        "MSL",
        "AGL",
        "N",
        "S",
        "E",
        "W",
        ",",
        ":",
        "+",
        "-",
        "=",
        "NONE",
        "47:30:00 N 011:00:00 E",
        "90:00:00 N 180:00:00 W",
        "FL95",
        "2026-11-19T12:00Z/NONE",
    ];
    for _ in 0..500 {
        let mut text = String::new();
        for _ in 0..random.usize(1..60) {
            text.push_str(commands[random.usize(0..commands.len())]);
            for _ in 0..random.usize(0..6) {
                text.push(' ');
                match random.u8(0..4) {
                    0 => text.push_str(words[random.usize(0..words.len())]),
                    1 => text.push_str(&random.u32(..).to_string()),
                    2 => text.push_str(&format!("{:e}", random.f64() * 1e300)),
                    _ => text.push_str(&format!("{:.3}", random.f64() * 1000.0)),
                }
            }
            text.push('\n');
        }
        read_soundly(&text);
    }
}

#[test]
fn a_large_hostile_file_is_read_in_bounded_time_and_memory() {
    // A megabyte in one line, and a hundred thousand lines of arcs, each of
    // which stands for hundreds of points.
    let one_line = "DP ".repeat(350_000);
    let file = read_soundly(&format!("AC R\nAN LONG\n{one_line}\n"));
    assert!(file.airspaces.is_empty());

    let mut text = String::from("AC R\nAN ARCS\nAL GND\nAH UNL\nV X=47:00:00N 011:00:00E\n");
    for _ in 0..100_000 {
        text.push_str("DA 500, 0, 359\n");
    }
    let started = std::time::Instant::now();
    let file = read_soundly(&text);
    assert!(file.airspaces.is_empty());
    assert_eq!(file.problems.len(), 1);
    assert!(file.problems[0].message.contains("more than"));
    // Generous: it takes a few hundredths of a second.
    assert!(started.elapsed().as_secs() < 20);
}
