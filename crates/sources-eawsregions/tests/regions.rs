//! What the crate makes of its asset, held to a golden file: the version,
//! each region with its English name, its label and the number of points
//! of its outline, and the region of a few take-offs. Rewritten with
//! `VENTI_UPDATE_GOLDEN=1` after the asset is made again; read the diff.

use std::path::Path;

use serde_json::json;
use venti_sources::testing::assert_golden;
use venti_sources_eawsregions::{region_at, regions, version};

#[test]
fn the_asset_is_read_into_the_same_regions() {
    let all: Vec<_> = regions()
        .iter()
        .map(|region| {
            json!({
                "id": region.id,
                "en": region.names.en,
                "label": [region.label.0, region.label.1],
                "polygons": region.polygons.len(),
                "points": region.polygons.iter().flatten().map(Vec::len).sum::<usize>(),
            })
        })
        .collect();
    // Take-offs west to east, and two places of the foreland.
    let places = [
        ("Planfait, Annecy", 6.2050, 45.8590),
        ("Fiesch", 8.1356, 46.4087),
        ("Niesen", 7.6511, 46.6450),
        ("Monte Cucco, Feltre", 11.8140, 45.9890),
        ("Brauneck", 11.5236, 47.6650),
        ("Hochfelln", 12.5650, 47.7570),
        ("Kössen, Unterberg", 12.4200, 47.6530),
        ("Greifenburg, Emberger Alm", 13.2010, 46.7580),
        ("Kobala", 13.6890, 46.1700),
        ("Munich", 11.5761, 48.1374),
        ("Milan", 9.1900, 45.4642),
    ];
    let places: Vec<_> = places
        .iter()
        .map(|&(name, lon, lat)| {
            json!({ "place": name, "region": region_at(lon, lat).map(|at| regions()[at].id.as_str()) })
        })
        .collect();
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/regions.json");
    assert_golden(
        &golden,
        &json!({ "version": version(), "regions": all, "places": places }),
    );
}
