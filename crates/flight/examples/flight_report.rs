//! Reads every flight file in a folder and says in a line what the crate
//! makes of it: for trying the reader and the analysis on real flights.
//!
//! ```text
//! cargo run --release -p venti-flight --example flight_report -- <folder of .igc files>
//! ```
//!
//! It prints no position: a flight file says where a person was. Keep such
//! files outside the repository.

use std::path::PathBuf;
use std::time::Instant;

use venti_flight::analysis::analyse;
use venti_flight::igc;
use venti_flight::scoring::RouteKind;
use venti_flight::summarise;

fn main() {
    let Some(folder) = std::env::args().nth(1) else {
        eprintln!("usage: flight_report <folder of .igc files>");
        std::process::exit(2);
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap_or_else(|error| {
            eprintln!("{folder}: {error}");
            std::process::exit(2);
        })
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("igc"))
        })
        .collect();
    files.sort();

    println!(
        "{:<28} {:>6} {:>7} {:>6} {:>7} {:>7} {:>6} {:>4} {:>6} {:<9} {:>7} {:>6}  notes",
        "file",
        "fixes",
        "record",
        "off at",
        "air",
        "track",
        "top",
        "th",
        "gain",
        "scored",
        "km",
        "ms"
    );
    let (mut read, mut failed) = (0, 0);
    let (mut airtime_s, mut scored_m) = (0i64, 0f64);
    for path in &files {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = match std::fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => {
                failed += 1;
                println!("{name:<28} cannot be read: {error}");
                continue;
            }
        };
        let began = Instant::now();
        let file = match igc::parse(&text) {
            Ok(file) => file,
            Err(error) => {
                failed += 1;
                println!("{name:<28} not a flight file the reader takes: {error:?}");
                continue;
            }
        };
        let Some(summary) = summarise(&file.fixes) else {
            failed += 1;
            println!("{name:<28} no fixes");
            continue;
        };
        let analysis = analyse(&file.fixes);
        let took_ms = began.elapsed().as_millis();
        read += 1;
        airtime_s += analysis.airtime_s;

        let mut notes: Vec<String> = Vec::new();
        if analysis.takeoff.is_none() {
            notes.push("no take-off found".to_owned());
        }
        if analysis.landing.is_none() {
            notes.push("ends in the air".to_owned());
        }
        if !file.has_pressure_altitude() {
            notes.push("no pressure altitude".to_owned());
        }
        let invalid = file.fixes.iter().filter(|fix| !fix.valid).count();
        if invalid > 0 {
            notes.push(format!("{invalid} fixes without a 3D fix"));
        }
        let (kind, km) = match &analysis.score {
            Some(score) => {
                scored_m += score.distance_m;
                let kind = match score.kind {
                    RouteKind::FreeFlight => "free",
                    RouteKind::FreeTriangle => "triangle",
                    RouteKind::FaiTriangle => "FAI",
                };
                (kind, format!("{:.1}", score.distance_m / 1000.0))
            }
            None => ("-", "-".to_owned()),
        };
        // How far into the recording the take-off was found.
        let off_at = analysis
            .takeoff
            .as_ref()
            .and_then(|takeoff| {
                let flown = venti_flight::analysis::flown_part(&file.fixes).first()?;
                let _ = takeoff;
                Some(clock((flown.time - summary.start).num_seconds()))
            })
            .unwrap_or_else(|| "-".to_owned());
        println!(
            "{:<28} {:>6} {:>7} {:>6} {:>7} {:>7.1} {:>6} {:>4} {:>6} {:<9} {:>7} {:>6}  {}",
            name,
            file.fixes.len(),
            clock(summary.duration_s()),
            off_at,
            clock(analysis.airtime_s),
            summary.track_length_m / 1000.0,
            summary.max_altitude_m,
            analysis.thermals.len(),
            analysis.height_gained_m,
            kind,
            km,
            took_ms,
            notes.join("; ")
        );
    }
    println!(
        "\n{read} flights read, {failed} not; {} in the air, {:.0} km scored",
        clock(airtime_s),
        scored_m / 1000.0
    );
}

/// Seconds as hours and minutes: "2:05".
fn clock(seconds: i64) -> String {
    format!("{}:{:02}", seconds / 3600, seconds % 3600 / 60)
}
