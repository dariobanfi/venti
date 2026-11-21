//! Reads the made-up feed and the device database through the adapter and
//! compares what it made with golden files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use venti_sources::testing::assert_golden;
use venti_sources::{Aircraft, LiveLine, LivePosition, LiveSource, Shown};
use venti_sources_ogn::{Ogn, fixtures};

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

#[derive(Default, Serialize)]
struct Read {
    lines: usize,
    server: usize,
    hidden: usize,
    other: usize,
    unreadable: BTreeMap<String, usize>,
    positions: usize,
    by_aircraft: BTreeMap<String, usize>,
    /// The newest position of every device.
    newest: BTreeMap<String, LivePosition>,
}

fn read(text: &str) -> Read {
    let ogn = Ogn::new();
    let mut read = Read::default();
    for line in text.lines() {
        read.lines += 1;
        match ogn.parse_line(line, fixtures::feed_at()) {
            LiveLine::Server => read.server += 1,
            LiveLine::Hidden => read.hidden += 1,
            LiveLine::Other(None) => read.other += 1,
            LiveLine::Other(Some(reason)) => *read.unreadable.entry(reason).or_default() += 1,
            LiveLine::Position(position) => {
                read.positions += 1;
                let kind = serde_json::to_value(position.aircraft).unwrap();
                *read
                    .by_aircraft
                    .entry(kind.as_str().unwrap().to_owned())
                    .or_default() += 1;
                let newer = read
                    .newest
                    .get(&position.device)
                    .is_none_or(|held| held.time <= position.time);
                if newer {
                    read.newest.insert(position.device.clone(), position);
                }
            }
        }
    }
    read
}

#[test]
fn the_made_up_feed_is_read_as_written() {
    let read = read(fixtures::feed());
    assert_eq!(read.lines, 23);
    assert_eq!(read.server, 3);
    // The no-tracking flag and the stealth flag.
    assert_eq!(read.hidden, 2);
    // The status lines, the receiver's position and the weather report.
    assert_eq!(read.other, 4);
    // The Naviter line and the time that is not hhmmss.
    assert_eq!(read.unreadable.values().sum::<usize>(), 2);
    assert_eq!(read.positions, 12);
    let newest = &read.newest["E00001"];
    assert_eq!(newest.aircraft, Aircraft::Paraglider);
    assert_eq!(newest.altitude_m, Some(2140.0));
    assert_eq!(newest.climb_ms, Some(1.8));
    assert_eq!(read.newest["E00002"].aircraft, Aircraft::Paraglider);
    assert_eq!(read.newest["E00003"].aircraft, Aircraft::HangGlider);
    assert_eq!(read.newest["E00007"].aircraft, Aircraft::Glider);
    assert_eq!(read.newest["E00008"].aircraft, Aircraft::TowPlane);
    assert_eq!(read.newest["E0000B"].aircraft, Aircraft::JetAircraft);
    // A FANET device on the ground: no course, speed or altitude.
    let ground = &read.newest["E0000C"];
    assert_eq!(ground.aircraft, Aircraft::Obstacle);
    assert_eq!((ground.altitude_m, ground.course_deg), (None, None));
    for position in read.newest.values() {
        assert!(fixtures::area().contains(position.position));
    }
    assert_golden(&golden("feed.json"), &read);
}

#[test]
fn the_made_up_directory_is_read_with_the_owners_choices() {
    let directory = Ogn::new().parse_directory(fixtures::directory()).unwrap();
    // The address of the wrong form is left out.
    assert_eq!(directory.devices.len(), 8);
    let not_tracked = directory.devices.values().filter(|d| !d.tracked).count();
    let anonymous = directory
        .devices
        .values()
        .filter(|d| d.tracked && !d.identified)
        .count();
    // E00004, E00010, E00011 (its second entry), E00012 (no flags); E00006.
    assert_eq!((not_tracked, anonymous), (4, 1));
    assert!(
        directory
            .devices
            .values()
            .filter(|d| !d.identified)
            .all(|d| d.registration.is_none() && d.competition_id.is_none() && d.model.is_none())
    );
    assert_golden(&golden("ddb.json"), &directory.devices);
}

#[test]
fn the_made_up_directory_says_who_is_shown_how() {
    let directory = Ogn::new().parse_directory(fixtures::directory()).unwrap();
    assert_eq!(directory.shown("E00004"), Shown::Not);
    assert_eq!(directory.shown("E00006"), Shown::Anonymous);
    // Not in the database: nobody chose, so nothing that identifies it.
    assert_eq!(directory.shown("E00002"), Shown::Anonymous);
    let Shown::Identified(glider) = directory.shown("E00007") else {
        panic!("the glider is identified");
    };
    assert_eq!(glider.registration.as_deref(), Some("D-0000"));
    assert_eq!(glider.competition_id.as_deref(), Some("TG"));
}

#[test]
fn a_damaged_line_is_something_else_with_why() {
    let ogn = Ogn::new();
    for line in [
        "FLRE00001>OGFLR,qAS,X:/1908h4739.84N\\01147.80E^210/014/A=007021 id1EE00001",
        "FLRE00001>OGFLR,qAS,X:/190850h47x9.84N\\01147.80E^210/014/A=007021 id1EE00001",
        "FLRE00001>OGFLR,qAS,X:/190850h4739.84Q\\01147.80E^210/014/A=007021 id1EE00001",
        "FLRE00001>OGFLR,qAS,X:/250850h4739.84N\\01147.80E^210/014/A=007021 id1EE00001",
        "FLRE00001>OGFLR,qAS,X:/190850h9939.84N\\01147.80E^210/014/A=007021 id1EE00001",
        "no colon at all",
    ] {
        assert!(
            matches!(
                ogn.parse_line(line, fixtures::feed_at()),
                LiveLine::Other(Some(_))
            ),
            "{line}"
        );
    }
}
