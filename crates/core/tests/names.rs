//! A record or an enum of the core becomes a type of the same name in the
//! iPhone app's `VentiCore` module. A type that is named like a framework
//! the app imports hides that framework's name wherever both are in scope:
//! on 2026-11-21 a record named `Observation` stopped the whole app from
//! compiling, because the macro that makes a model observable writes
//! `Observation.ObservationRegistrar` and found the record instead of
//! Apple's Observation framework. This test keeps such a name out.

use std::fs;
use std::path::Path;

/// The frameworks the iPhone app imports, and a few it is likely to: none
/// may be the name of a type the core exports.
const FRAMEWORKS: &[&str] = &[
    "Observation",
    "Foundation",
    "SwiftUI",
    "Swift",
    "UIKit",
    "CoreLocation",
    "MapLibre",
    "Charts",
    "Security",
    "AVFoundation",
    "VisionKit",
    "Vision",
    "UniformTypeIdentifiers",
    "Network",
    "CoreMotion",
    "CoreImage",
    "CoreBluetooth",
    "Combine",
    "Testing",
    "XCTest",
    "WidgetKit",
    "ActivityKit",
    "Photos",
    "MapKit",
];

#[test]
fn no_type_of_the_core_is_named_like_a_framework_of_the_iphone_app() {
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut clashes = Vec::new();
    for entry in fs::read_dir(&sources).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        for line in text.lines() {
            let line = line.trim_start();
            let name = ["pub struct ", "pub enum ", "pub trait ", "pub type "]
                .iter()
                .find_map(|keyword| line.strip_prefix(keyword))
                .map(|rest| {
                    rest.chars()
                        .take_while(|letter| letter.is_alphanumeric() || *letter == '_')
                        .collect::<String>()
                });
            if let Some(name) = name
                && FRAMEWORKS.contains(&name.as_str())
            {
                clashes.push(format!("{name} in {}", path.display()));
            }
        }
    }
    assert!(
        clashes.is_empty(),
        "types named like a framework the iPhone app imports: {clashes:?}"
    );
}
