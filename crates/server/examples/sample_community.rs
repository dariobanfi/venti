//! Writes the seven made-up sample pilots into a folder, for
//! `tools/sample-community/seed.py` to put into a running server: their
//! pictures, their flight files as the generator makes them, and
//! `sample-community.json`, which says who they are, which public groups
//! they are in and what they wrote there.
//!
//! ```text
//! cargo run -p venti-server --example sample_community -- <folder>
//! ```

use std::path::PathBuf;

use serde_json::json;
use venti_server::dev::DEMO_COMMUNITIES;
use venti_server::dev::sample_pilots::{SAMPLE_PILOTS, flights};

fn main() -> Result<(), String> {
    let folder: PathBuf = std::env::args_os()
        .nth(1)
        .ok_or("name the folder to write into")?
        .into();
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let write = |name: &str, bytes: &[u8]| {
        let path = folder.join(name);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    };

    let mut pilots = Vec::new();
    for (index, pilot) in SAMPLE_PILOTS.iter().enumerate() {
        let picture = format!("{}.png", pilot.handle);
        write(&picture, pilot.picture)?;
        let mut files = Vec::new();
        for (number, file) in flights(index, pilot).iter().enumerate() {
            let name = format!("{}-{}.igc", pilot.handle, number + 1);
            write(&name, file)?;
            files.push(name);
        }
        let group = |at: usize| DEMO_COMMUNITIES[at].name;
        pilots.push(json!({
            "handle": pilot.handle,
            "display_name": pilot.name,
            "country": pilot.country,
            "picture": picture,
            "flights": files,
            "groups": pilot.groups.iter().map(|(at, _)| group(*at)).collect::<Vec<_>>(),
            "messages": pilot.messages.iter().map(|(at, minutes_ago, text)| json!({
                "group": group(*at),
                "minutes_ago": minutes_ago,
                "text": text,
            })).collect::<Vec<_>>(),
        }));
    }
    let groups: Vec<_> = DEMO_COMMUNITIES
        .iter()
        .map(|group| {
            let (lat, lon, radius_km, place) = group.home;
            json!({
                "name": group.name,
                "description": group.description,
                "home": {
                    "center": {"lat": lat, "lon": lon},
                    "radius_km": radius_km,
                    "place": place,
                },
            })
        })
        .collect();
    let manifest = json!({"pilots": pilots, "groups": groups});
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    write("sample-community.json", text.as_bytes())?;
    println!("{}", folder.display());
    Ok(())
}
